use dartel_image::{Image, MAX_SIDE};

#[test]
fn rgba_pixels_are_kept_as_given() {
    let image = Image::from_rgba(1, 2, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        image,
        Some(Image {
            width: 1,
            height: 2,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        })
    );
}

#[test]
fn rgba_pixels_of_the_wrong_length_are_rejected() {
    assert_eq!(Image::from_rgba(1, 2, vec![0; 7]), None);
}

#[test]
fn rgba_image_without_columns_is_rejected() {
    assert_eq!(Image::from_rgba(0, 1, Vec::new()), None);
}

#[test]
fn rgba_image_without_rows_is_rejected() {
    assert_eq!(Image::from_rgba(1, 0, Vec::new()), None);
}

#[test]
fn rgba_image_wider_than_the_limit_is_rejected() {
    let width = MAX_SIDE + 1;
    assert_eq!(
        Image::from_rgba(width, 1, vec![0; width as usize * 4]),
        None
    );
}

#[test]
fn rgba_image_taller_than_the_limit_is_rejected() {
    let height = MAX_SIDE + 1;
    assert_eq!(
        Image::from_rgba(1, height, vec![0; height as usize * 4]),
        None
    );
}

#[test]
fn rgb_pixels_become_opaque() {
    let image = Image::from_rgb(2, 1, &[1, 2, 3, 4, 5, 6]);
    assert_eq!(
        image.map(|image| image.pixels),
        Some(vec![1, 2, 3, 255, 4, 5, 6, 255])
    );
}

#[test]
fn rgb_pixels_of_the_wrong_length_are_rejected() {
    assert_eq!(Image::from_rgb(2, 1, &[0; 5]), None);
}

#[test]
fn rgb_image_without_columns_is_rejected() {
    assert_eq!(Image::from_rgb(0, 1, &[]), None);
}
