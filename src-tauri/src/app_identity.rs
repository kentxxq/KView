use tauri::{Context, Wry};
#[cfg(debug_assertions)]
use tauri::image::Image;

pub(crate) const APP_NAME: &str = if cfg!(debug_assertions) { "KView DEV" } else { "KView" };

pub(crate) fn configure(context: &mut Context<Wry>) {
    #[cfg(debug_assertions)]
    {
        // 在初始化单实例插件及系统路径前隔离开发版，避免唤醒已安装的正式版。
        context.config_mut().identifier = "com.kentxxq.kview.dev".into();
        context.config_mut().product_name = Some(APP_NAME.into());
        if let Some(icon) = context.default_window_icon() {
            context.set_default_window_icon(Some(dev_icon(icon)));
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = context;
}

#[cfg(debug_assertions)]
fn dev_icon(icon: &Image<'_>) -> Image<'static> {
    let width = icon.width() as usize;
    let height = icon.height() as usize;
    let mut rgba = icon.rgba().to_vec();
    let scale = (width.min(height) / 16).max(1);
    let badge_width = 13 * scale;
    let badge_height = 7 * scale;
    let left = width - badge_width;
    let top = height - badge_height;
    // 橙色角标与三个 3×5 像素字母，直接复用已有图标，不增加图片依赖。
    for y in top..height {
        for x in left..width {
            let offset = (y * width + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&[245, 158, 11, 255]);
        }
    }
    let letters = [
        [0b110, 0b101, 0b101, 0b101, 0b110], // D
        [0b111, 0b100, 0b110, 0b100, 0b111], // E
        [0b101, 0b101, 0b101, 0b101, 0b010], // V
    ];
    for (letter, rows) in letters.iter().enumerate() {
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) == 0 { continue; }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = left + (1 + letter * 4 + column) * scale + dx;
                        let y = top + (1 + row) * scale + dy;
                        let offset = (y * width + x) * 4;
                        rgba[offset..offset + 4].copy_from_slice(&[17, 24, 39, 255]);
                    }
                }
            }
        }
    }
    Image::new_owned(rgba, icon.width(), icon.height())
}
