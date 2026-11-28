use dartel_image::Image;

fn image(width: u32, height: u32, pixels: &[u8]) -> Image {
    Image {
        width,
        height,
        pixels: pixels.to_vec(),
    }
}

#[test]
fn opaque_pixels_keep_their_color_at_the_same_size() {
    let source = image(2, 1, &[10, 20, 30, 255, 40, 50, 60, 255]);
    assert_eq!(
        source.premultiplied([0, 0, 2, 1], 2, 1),
        [10, 20, 30, 255, 40, 50, 60, 255]
    );
}

#[test]
fn translucent_color_is_multiplied_by_its_alpha() {
    let source = image(1, 1, &[255, 0, 0, 128]);
    assert_eq!(source.premultiplied([0, 0, 1, 1], 1, 1), [128, 0, 0, 128]);
}

#[test]
fn shrinking_averages_the_covered_pixels() {
    let source = image(
        2,
        2,
        &[
            255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255,
        ],
    );
    assert_eq!(
        source.premultiplied([0, 0, 2, 2], 1, 1),
        [128, 128, 128, 255]
    );
}

#[test]
fn transparent_pixels_add_no_color_to_an_average() {
    let source = image(2, 1, &[255, 0, 0, 255, 0, 255, 0, 0]);
    assert_eq!(source.premultiplied([0, 0, 2, 1], 1, 1), [128, 0, 0, 128]);
}

#[test]
fn enlarging_repeats_the_nearest_pixel() {
    let source = image(2, 1, &[10, 20, 30, 255, 40, 50, 60, 255]);
    assert_eq!(
        source.premultiplied([0, 0, 2, 1], 4, 2),
        [
            10, 20, 30, 255, 10, 20, 30, 255, 40, 50, 60, 255, 40, 50, 60, 255, 10, 20, 30, 255,
            10, 20, 30, 255, 40, 50, 60, 255, 40, 50, 60, 255,
        ]
    );
}

#[test]
fn source_rectangle_selects_part_of_the_image() {
    let source = image(
        2,
        2,
        &[1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255],
    );
    assert_eq!(source.premultiplied([1, 1, 1, 1], 1, 1), [4, 4, 4, 255]);
}
