//! The pictures a user gives a Wardrobe card from a file or the clipboard
//! (docs/WARDROBE.md, section 10). They stay on the PC, in the account's
//! `wardrobe-pictures` folder: nothing is uploaded and no texture is
//! created on the grid. A picture is read, fitted in 512 pixels and saved
//! as a PNG by a worker; the demo, which writes nothing, keeps it in
//! memory.

use crate::world::wardrobe;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The longest side kept, and the largest file read.
const SIDE: u32 = 512;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

enum Slot {
    Loading(crossbeam_channel::Receiver<Option<egui::ColorImage>>),
    Ready(egui::TextureHandle),
    Failed,
}

static TEXTURES: Mutex<Option<HashMap<String, Slot>>> = Mutex::new(None);
/// The demo's pictures, by name.
static MEMORY: Mutex<Option<HashMap<String, Arc<egui::ColorImage>>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A name this module gave: nothing else is ever read from the folder,
/// whatever an account file says.
fn valid(name: &str) -> bool {
    name.strip_suffix(".png")
        .is_some_and(|stem| stem.len() == 36 && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
}

fn color_image(image: &image::RgbaImage) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw())
}

/// The texture of a chosen picture, read off the frame thread the first
/// time it is asked for; `None` until it is there.
pub(super) fn texture(ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
    if !valid(name) {
        return None;
    }
    let mut guard = lock(&TEXTURES);
    let slots = guard.get_or_insert_with(HashMap::new);
    match slots.get(name) {
        Some(Slot::Ready(texture)) => return Some(texture.clone()),
        Some(Slot::Failed) => return None,
        Some(Slot::Loading(done)) => {
            let Ok(read) = done.try_recv() else { return None };
            let slot = match read {
                Some(image) => Slot::Ready(ctx.load_texture(format!("wardrobe-picture-{name}"), image, egui::TextureOptions::LINEAR)),
                None => Slot::Failed,
            };
            slots.insert(name.to_owned(), slot);
            ctx.request_repaint();
            return None;
        }
        None => {}
    }
    let (tx, done) = crossbeam_channel::bounded(1);
    if let Some(image) = lock(&MEMORY).as_ref().and_then(|m| m.get(name).cloned()) {
        let _ = tx.send(Some((*image).clone()));
    } else if let Some(folder) = wardrobe::with(|shared| shared.pictures.clone()) {
        let path = folder.join(name);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("aurora-wardrobe-picture".into()).spawn(move || {
            let read = std::fs::metadata(&path)
                .ok()
                .filter(|meta| meta.len() <= MAX_BYTES)
                .and_then(|_| std::fs::read(&path).ok())
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|image| color_image(&image.to_rgba8()));
            let _ = tx.send(read);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            slots.insert(name.to_owned(), Slot::Failed);
            return None;
        }
    } else {
        slots.insert(name.to_owned(), Slot::Failed);
        return None;
    }
    slots.insert(name.to_owned(), Slot::Loading(done));
    None
}

/// Where a new picture comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum From {
    File,
    Clipboard,
}

/// A picture being chosen: the name of its file once it is saved, `None`
/// when the user cancelled, or why it failed.
pub(super) type Job = crossbeam_channel::Receiver<Result<Option<String>, String>>;

fn fitted(image: image::DynamicImage) -> image::RgbaImage {
    if image.width().max(image.height()) > SIDE {
        image.thumbnail(SIDE, SIDE).to_rgba8()
    } else {
        image.to_rgba8()
    }
}

fn from_file() -> Result<Option<image::DynamicImage>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(
            aurora_i18n::tr!("wardrobe-picture-filter"),
            &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tga"],
        )
        .pick_file()
    else {
        return Ok(None);
    };
    let unreadable = || aurora_i18n::tr!("wardrobe-picture-unreadable").to_owned();
    if std::fs::metadata(&path).map_err(|_| unreadable())?.len() > MAX_BYTES {
        return Err(aurora_i18n::tr!("wardrobe-picture-too-large").to_owned());
    }
    let bytes = std::fs::read(&path).map_err(|_| unreadable())?;
    image::load_from_memory(&bytes).map(Some).map_err(|_| unreadable())
}

fn from_clipboard() -> Result<Option<image::DynamicImage>, String> {
    let empty = || aurora_i18n::tr!("wardrobe-picture-no-clipboard").to_owned();
    let mut board = arboard::Clipboard::new().map_err(|_| empty())?;
    let copied = board.get_image().map_err(|_| empty())?;
    let (width, height) = (copied.width as u32, copied.height as u32);
    image::RgbaImage::from_raw(width, height, copied.bytes.into_owned())
        .map(|image| Some(image::DynamicImage::ImageRgba8(image)))
        .ok_or_else(empty)
}

/// Chooses a picture off the frame thread: the file dialog blocks, and so
/// may the clipboard.
pub(super) fn choose(ctx: &egui::Context, from: From) -> Job {
    let (tx, job) = crossbeam_channel::bounded(1);
    let folder = wardrobe::with(|shared| shared.pictures.clone());
    let ctx = ctx.clone();
    let failed = tx.clone();
    let spawned = std::thread::Builder::new().name("aurora-wardrobe-picture".into()).spawn(move || {
        let chosen = match from {
            From::File => from_file(),
            From::Clipboard => from_clipboard(),
        };
        let saved = chosen.and_then(|image| {
            let Some(image) = image else { return Ok(None) };
            let image = fitted(image);
            let name = format!("{}.png", uuid::Uuid::new_v4());
            match &folder {
                Some(folder) => {
                    let mut bytes = Vec::new();
                    image::DynamicImage::ImageRgba8(image)
                        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
                        .map_err(|error| error.to_string())?;
                    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
                    crate::atomic_file::write(&folder.join(&name), &bytes).map_err(|error| error.to_string())?;
                }
                None => {
                    lock(&MEMORY)
                        .get_or_insert_with(HashMap::new)
                        .insert(name.clone(), Arc::new(color_image(&image)));
                }
            }
            Ok(Some(name))
        });
        let _ = tx.send(saved);
        ctx.request_repaint();
    });
    if let Err(error) = spawned {
        let _ = failed.send(Err(error.to_string()));
    }
    job
}

/// Forgets a picture that no card shows any more, and deletes its file.
pub(super) fn discard(name: &str) {
    if !valid(name) {
        return;
    }
    if let Some(slots) = lock(&TEXTURES).as_mut() {
        slots.remove(name);
    }
    if let Some(memory) = lock(&MEMORY).as_mut() {
        memory.remove(name);
    }
    if let Some(folder) = wardrobe::with(|shared| shared.pictures.clone()) {
        let path: PathBuf = folder.join(name);
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("Cannot delete a Wardrobe picture: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_names_this_module_gives_are_read() {
        assert!(valid(&format!("{}.png", uuid::Uuid::new_v4())));
        for name in [
            "../../secret.png",
            "a.png",
            "x/y.png",
            "00000000-0000-0000-0000-00000000000g.png",
            "picture.jpg",
            "",
        ] {
            assert!(!valid(name), "{name}");
        }
    }

    #[test]
    fn a_large_picture_is_fitted_and_a_small_one_kept() {
        let large = image::DynamicImage::ImageRgba8(image::RgbaImage::new(2048, 1024));
        let fitted_large = fitted(large);
        assert_eq!((fitted_large.width(), fitted_large.height()), (512, 256));
        let small = image::DynamicImage::ImageRgba8(image::RgbaImage::new(200, 300));
        let fitted_small = fitted(small);
        assert_eq!((fitted_small.width(), fitted_small.height()), (200, 300));
    }
}
