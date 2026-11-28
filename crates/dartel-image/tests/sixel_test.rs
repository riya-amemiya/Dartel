use dartel_image::Image;
use dartel_image::sixel::decode;

const BLACK: [u8; 4] = [0, 0, 0, 255];
const BLUE: [u8; 4] = [51, 51, 204, 255];
const RED: [u8; 4] = [204, 33, 33, 255];
const CLEAR: [u8; 4] = [0; 4];

fn image(width: u32, height: u32, pixels: &[[u8; 4]]) -> Image {
    Image {
        width,
        height,
        pixels: pixels.concat(),
    }
}

fn color(data: &[u8]) -> Option<[u8; 4]> {
    decode(data, None).and_then(|image| image.pixels.first_chunk::<4>().copied())
}

fn size(data: &[u8]) -> Option<(u32, u32)> {
    decode(data, None).map(|image| (image.width, image.height))
}

#[test]
fn full_character_paints_a_column_of_six_pixels() {
    assert_eq!(decode(b"~", None), Some(image(1, 6, &[BLACK; 6])));
}

#[test]
fn lowest_bit_is_the_top_pixel() {
    assert_eq!(decode(b"@", None), Some(image(1, 1, &[BLACK])));
}

#[test]
fn highest_bit_is_the_bottom_pixel() {
    assert_eq!(
        decode(b"_", None),
        Some(image(1, 6, &[CLEAR, CLEAR, CLEAR, CLEAR, CLEAR, BLACK]))
    );
}

#[test]
fn color_introducer_with_one_parameter_selects_a_register() {
    assert_eq!(color(b"#1@"), Some(BLUE));
}

#[test]
fn registers_past_the_sixteenth_repeat_the_default_colors() {
    assert_eq!(color(b"#17@"), Some(BLUE));
}

#[test]
fn register_numbers_wrap_at_the_register_count() {
    assert_eq!(color(b"#257@"), Some(BLUE));
}

#[test]
fn repeat_introducer_paints_several_columns() {
    assert_eq!(decode(b"!3@", None), Some(image(3, 1, &[BLACK; 3])));
}

#[test]
fn repeat_count_of_zero_paints_one_column() {
    assert_eq!(decode(b"!0@", None), Some(image(1, 1, &[BLACK])));
}

#[test]
fn carriage_return_overprints_the_same_band() {
    assert_eq!(
        decode(b"#1~$#2@", None),
        Some(image(1, 6, &[RED, BLUE, BLUE, BLUE, BLUE, BLUE]))
    );
}

#[test]
fn line_feed_moves_down_six_pixels() {
    assert_eq!(
        decode(b"@-@", None),
        Some(image(
            1,
            7,
            &[BLACK, CLEAR, CLEAR, CLEAR, CLEAR, CLEAR, BLACK]
        ))
    );
}

#[test]
fn rgb_definition_scales_percentages() {
    assert_eq!(color(b"#5;2;100;0;50@"), Some([255, 0, 128, 255]));
}

#[test]
fn rgb_percentages_above_one_hundred_are_clamped() {
    assert_eq!(color(b"#5;2;200;0;0@"), Some([255, 0, 0, 255]));
}

#[test]
fn hls_hue_zero_is_blue() {
    assert_eq!(color(b"#5;1;0;50;100@"), Some([0, 0, 255, 255]));
}

#[test]
fn hls_hue_sixty_is_magenta() {
    assert_eq!(color(b"#5;1;60;50;100@"), Some([255, 0, 255, 255]));
}

#[test]
fn hls_hue_one_hundred_twenty_is_red() {
    assert_eq!(color(b"#5;1;120;50;100@"), Some([255, 0, 0, 255]));
}

#[test]
fn hls_hue_one_hundred_eighty_is_yellow() {
    assert_eq!(color(b"#5;1;180;50;100@"), Some([255, 255, 0, 255]));
}

#[test]
fn hls_hue_two_hundred_forty_is_green() {
    assert_eq!(color(b"#5;1;240;50;100@"), Some([0, 255, 0, 255]));
}

#[test]
fn hls_hue_three_hundred_is_cyan() {
    assert_eq!(color(b"#5;1;300;50;100@"), Some([0, 255, 255, 255]));
}

#[test]
fn hls_definition_mixes_lightness_and_saturation() {
    assert_eq!(color(b"#5;1;120;46;72@"), Some([202, 33, 33, 255]));
}

#[test]
fn hls_lightness_above_one_hundred_is_clamped() {
    assert_eq!(color(b"#5;1;120;200;100@"), Some([255, 255, 255, 255]));
}

#[test]
fn hls_saturation_above_one_hundred_is_clamped() {
    assert_eq!(color(b"#5;1;120;50;200@"), Some([255, 0, 0, 255]));
}

#[test]
fn unknown_color_space_leaves_the_register_unchanged() {
    assert_eq!(color(b"#1;3;10;20;30@"), Some(BLUE));
}

#[test]
fn definition_with_too_few_parameters_only_selects() {
    assert_eq!(color(b"#1;2;100@"), Some(BLUE));
}

#[test]
fn definition_with_too_many_parameters_only_selects() {
    assert_eq!(color(b"#1;2;100;0;0;99@"), Some(BLUE));
}

#[test]
fn raster_attributes_declare_the_image_size() {
    assert_eq!(
        decode(b"\"1;1;2;2@", None),
        Some(image(2, 2, &[BLACK, CLEAR, CLEAR, CLEAR]))
    );
}

#[test]
fn backdrop_fills_the_pixels_left_unpainted() {
    assert_eq!(
        decode(b"\"1;1;2;1@", Some([9, 8, 7])),
        Some(image(2, 1, &[BLACK, [9, 8, 7, 255]]))
    );
}

#[test]
fn empty_character_advances_without_painting() {
    assert_eq!(decode(b"?@", None), Some(image(2, 1, &[CLEAR, BLACK])));
}

#[test]
fn bytes_outside_the_sixel_set_are_skipped() {
    assert_eq!(decode(b"@\n@", None), Some(image(2, 1, &[BLACK; 2])));
}

#[test]
fn data_without_pixels_is_rejected() {
    assert_eq!(decode(b"", None), None);
}

#[test]
fn data_ending_inside_a_parameter_keeps_the_pixels_before_it() {
    assert_eq!(decode(b"@#1", None), Some(image(1, 1, &[BLACK])));
}

#[test]
fn repeat_past_the_width_limit_is_clipped() {
    assert_eq!(size(b"!5000@"), Some((4096, 1)));
}

#[test]
fn character_past_the_width_limit_is_dropped() {
    assert_eq!(size(b"!4096@@"), Some((4096, 1)));
}

#[test]
fn bands_past_the_height_limit_are_clipped() {
    assert_eq!(size("~-".repeat(684).as_bytes()), Some((1, 4096)));
}

#[test]
fn declared_width_past_the_limit_is_clamped() {
    assert_eq!(size(b"\"1;1;5000;1@"), Some((4096, 1)));
}

#[test]
fn declared_height_past_the_limit_is_clamped() {
    assert_eq!(size(b"\"1;1;1;5000@"), Some((1, 4096)));
}
