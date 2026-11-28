use dartel_term::{Term, Tile};
use dartel_vt::Parser;

const COLOR: [u8; 3] = [200, 100, 50];
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xB6, 0x0D,
    0x24, 0x00, 0x00, 0x00, 0x19, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x05, 0xC1, 0x01, 0x01, 0x00,
    0x00, 0x08, 0x80, 0x20, 0x7A, 0xD6, 0xB4, 0x9E, 0x1B, 0x88, 0xE4, 0x86, 0x94, 0x7D, 0x3D, 0x23,
    0x06, 0xBC, 0x93, 0xB0, 0x33, 0x6B, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42,
    0x60, 0x82,
];
const PNG_PIXELS: [u8; 16] = [255, 0, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0, 64, 64, 64, 64];
const MISSING: &str = "ENOENT:no image with this id";

fn term(lines: usize, cols: usize) -> Term {
    let mut term = Term::new(lines, cols, 100);
    term.set_cell_size(10, 20);
    term
}

fn feed(term: &mut Term, input: &[u8]) {
    Parser::new().advance(term, input);
}

fn fed(lines: usize, cols: usize, inputs: &[&[u8]]) -> Term {
    let mut term = term(lines, cols);
    for input in inputs {
        feed(&mut term, input);
    }
    term
}

fn reply(term: &mut Term) -> String {
    String::from_utf8(term.take_reply()).unwrap()
}

fn answered(inputs: &[&[u8]]) -> String {
    reply(&mut fed(6, 8, inputs))
}

fn answer(id: u32, message: &str) -> String {
    format!("\x1b_Gi={id};{message}\x1b\\")
}

fn cursor(term: &Term) -> (usize, usize) {
    (term.cursor().line, term.cursor().column)
}

fn encoded(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    bytes
        .chunks(3)
        .flat_map(|chunk| {
            let value = chunk
                .iter()
                .fold(0_u32, |value, &byte| value << 8 | u32::from(byte))
                << (8 * (3 - chunk.len()));
            (0..4).map(move |index| {
                if index <= chunk.len() {
                    char::from(ALPHABET[(value >> (18 - 6 * index) & 63) as usize])
                } else {
                    '='
                }
            })
        })
        .collect()
}

fn deflated(data: &[u8]) -> Vec<u8> {
    let (low, high) = data.iter().fold((1_u32, 0_u32), |(low, high), &byte| {
        (
            (low + u32::from(byte)) % 65521,
            (high + low + u32::from(byte)) % 65521,
        )
    });
    [
        &[0x78, 0x01, 0x01][..],
        &(data.len() as u16).to_le_bytes(),
        &(!(data.len() as u16)).to_le_bytes(),
        data,
        &(high << 16 | low).to_be_bytes(),
    ]
    .concat()
}

fn kitty(control: &str, data: &[u8]) -> Vec<u8> {
    format!("\x1b_G{control};{}\x1b\\", encoded(data)).into_bytes()
}

fn solid(control: &str, width: usize, height: usize) -> Vec<u8> {
    kitty(
        &format!("f=24,s={width},v={height},{control}"),
        &COLOR.repeat(width * height),
    )
}

fn checkered(control: &str) -> Vec<u8> {
    let pixels: Vec<u8> = (0..4_u8)
        .flat_map(|y| (0..4).flat_map(move |x| [x * 10, y * 10, 0]))
        .collect();
    kitty(&format!("f=24,s=4,v=4,{control}"), &pixels)
}

fn sixel(data: &str) -> Vec<u8> {
    format!("\x1bPq{data}\x1b\\").into_bytes()
}

fn tiles(term: &Term, line: usize) -> &[Tile] {
    term.grid().row(line).tiles()
}

fn shape(tile: &Tile) -> (u16, u32, u32, u32) {
    (tile.column, tile.y, tile.width, tile.height)
}

fn layout(term: &Term) -> Vec<Vec<(u16, u32, u32, u32)>> {
    (0..term.lines())
        .map(|line| tiles(term, line).iter().map(shape).collect())
        .collect()
}

fn cuts(term: &Term, line: usize) -> Vec<(u16, u32, u32)> {
    tiles(term, line)
        .iter()
        .map(|tile| (tile.column, tile.x, tile.width))
        .collect()
}

fn counts(term: &Term) -> Vec<usize> {
    (0..term.lines())
        .map(|line| tiles(term, line).len())
        .collect()
}

fn size(term: &Term) -> (u32, u32) {
    let picture = &tiles(term, 0)[0].picture;
    (picture.width, picture.height)
}

fn pair() -> Term {
    let mut term = fed(
        6,
        8,
        &[
            &solid("a=T,i=1", 10, 20),
            b"\x1b[3;4H",
            &solid("a=T,i=2", 10, 20),
        ],
    );
    term.take_reply();
    term
}

fn deleted(control: &str) -> Vec<usize> {
    let mut term = pair();
    feed(&mut term, &kitty(control, &[]));
    counts(&term)
}

fn kept(term: &mut Term, id: u32) -> bool {
    feed(term, &kitty(&format!("a=p,i={id},C=1"), &[]));
    reply(term) == answer(id, "OK")
}

#[test]
fn displayed_image_is_cut_into_one_tile_per_row() {
    let term = fed(6, 8, &[&solid("a=T", 30, 50)]);
    assert_eq!(
        layout(&term),
        [
            vec![(0, 0, 30, 20)],
            vec![(0, 20, 30, 20)],
            vec![(0, 40, 30, 10)],
            vec![],
            vec![],
            vec![]
        ]
    );
}

#[test]
fn cursor_ends_on_the_last_image_row_past_its_last_column() {
    let term = fed(6, 8, &[&solid("a=T", 30, 50)]);
    assert_eq!(cursor(&term), (2, 3));
}

#[test]
fn cursor_policy_one_leaves_the_cursor_in_place() {
    let term = fed(6, 8, &[&solid("a=T,C=1", 30, 50)]);
    assert_eq!(cursor(&term), (0, 0));
}

#[test]
fn image_starts_at_the_cursor_cell() {
    let term = fed(6, 8, &[b"\x1b[2;3H", &solid("a=T", 10, 20)]);
    assert_eq!(layout(&term)[1], [(2, 0, 10, 20)]);
}

#[test]
fn image_reaching_the_right_edge_moves_the_cursor_to_the_next_line() {
    let term = fed(6, 3, &[&solid("a=T", 30, 20)]);
    assert_eq!(cursor(&term), (1, 0));
}

#[test]
fn image_wider_than_the_remaining_columns_is_cut_off() {
    let term = fed(6, 2, &[&solid("a=T", 30, 20)]);
    assert_eq!(layout(&term)[0], [(0, 0, 20, 20)]);
}

#[test]
fn image_taller_than_the_screen_scrolls_it() {
    let term = fed(2, 8, &[&solid("a=T", 10, 60)]);
    assert_eq!(
        (
            term.grid().absolute(0).tiles().iter().map(shape).collect(),
            layout(&term)
        ),
        (
            vec![(0, 0, 10, 20)],
            vec![vec![(0, 20, 10, 20)], vec![(0, 40, 10, 20)]]
        )
    );
}

#[test]
fn cursor_policy_one_clips_the_image_at_the_bottom() {
    let term = fed(2, 8, &[&solid("a=T,C=1", 10, 60)]);
    assert_eq!(
        (term.grid().history(), layout(&term)),
        (0, vec![vec![(0, 0, 10, 20)], vec![(0, 20, 10, 20)]])
    );
}

#[test]
fn image_below_the_scroll_region_stops_at_the_last_row() {
    let term = fed(6, 8, &[b"\x1b[1;2r\x1b[6;1H", &solid("a=T", 10, 60)]);
    assert_eq!(counts(&term), [0, 0, 0, 0, 0, 1]);
}

#[test]
fn image_id_is_acknowledged() {
    assert_eq!(answered(&[&solid("a=t,i=7", 1, 1)]), answer(7, "OK"));
}

#[test]
fn command_without_an_id_is_not_answered() {
    assert_eq!(answered(&[&solid("a=t", 1, 1)]), "");
}

#[test]
fn quiet_level_one_drops_the_acknowledgement() {
    assert_eq!(answered(&[&solid("a=t,i=7,q=1", 1, 1)]), "");
}

#[test]
fn quiet_level_one_keeps_errors() {
    assert_eq!(answered(&[&kitty("a=p,i=7,q=1", &[])]), answer(7, MISSING));
}

#[test]
fn quiet_level_two_drops_errors() {
    assert_eq!(answered(&[&kitty("a=p,i=7,q=2", &[])]), "");
}

#[test]
fn image_number_is_answered_with_a_new_id() {
    assert_eq!(
        answered(&[&solid("a=t,I=13", 1, 1)]),
        "\x1b_Gi=4294967295,I=13;OK\x1b\\"
    );
}

#[test]
fn id_together_with_a_number_is_rejected() {
    assert_eq!(
        answered(&[&solid("a=t,i=1,I=2", 1, 1)]),
        "\x1b_Gi=1,I=2;EINVAL:an image id and an image number were both given\x1b\\"
    );
}

#[test]
fn placement_id_is_echoed_when_an_image_is_displayed() {
    assert_eq!(
        answered(&[&solid("a=T,i=7,p=3", 1, 1)]),
        "\x1b_Gi=7,p=3;OK\x1b\\"
    );
}

#[test]
fn placement_id_is_left_out_when_an_image_is_only_stored() {
    assert_eq!(answered(&[&solid("a=t,i=7,p=3", 1, 1)]), answer(7, "OK"));
}

#[test]
fn query_checks_the_data_without_keeping_the_image() {
    assert_eq!(
        answered(&[&solid("a=q,i=31", 1, 1), &kitty("a=p,i=31", &[])]),
        answer(31, "OK") + &answer(31, MISSING)
    );
}

#[test]
fn stored_image_is_not_displayed() {
    let term = fed(6, 8, &[&solid("a=t,i=5", 10, 20)]);
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn stored_image_is_displayed_by_a_later_put() {
    let term = fed(6, 8, &[&solid("a=t,i=5", 10, 20), &kitty("a=p,i=5", &[])]);
    assert_eq!(layout(&term)[0], [(0, 0, 10, 20)]);
}

#[test]
fn put_by_number_shows_the_newest_image_with_that_number() {
    let term = fed(
        6,
        8,
        &[
            &solid("a=t,I=4", 10, 20),
            &solid("a=t,I=4", 20, 20),
            &kitty("a=p,I=4", &[]),
        ],
    );
    assert_eq!(layout(&term)[0], [(0, 0, 20, 20)]);
}

#[test]
fn put_by_an_unknown_number_reports_a_missing_image() {
    assert_eq!(
        answered(&[&kitty("a=p,I=5", &[])]),
        format!("\x1b_Gi=0,I=5;{MISSING}\x1b\\")
    );
}

#[test]
fn command_without_a_payload_separator_is_understood() {
    assert_eq!(answered(&[b"\x1b_Ga=p,i=3\x1b\\"]), answer(3, MISSING));
}

#[test]
fn control_entries_without_a_value_are_skipped() {
    assert_eq!(answered(&[b"\x1b_Ga=p,junk,i=3\x1b\\"]), answer(3, MISSING));
}

#[test]
fn control_values_that_are_not_numbers_count_as_zero() {
    assert_eq!(
        answered(&[b"\x1b_Ga=p,i=3,q=loud\x1b\\"]),
        answer(3, MISSING)
    );
}

#[test]
fn unknown_control_keys_are_skipped() {
    assert_eq!(answered(&[b"\x1b_Ga=p,i=3,J=1\x1b\\"]), answer(3, MISSING));
}

#[test]
fn application_strings_for_other_programs_are_ignored() {
    assert_eq!(answered(&[b"\x1b_Xa=p,i=3\x1b\\"]), "");
}

#[test]
fn chunked_data_is_joined_before_decoding() {
    let data = encoded(&COLOR.repeat(3));
    let term = fed(
        6,
        8,
        &[format!(
            "\x1b_Ga=T,f=24,s=3,v=1,m=1;{}\x1b\\\x1b_Gm=1;{}\x1b\\\x1b_Gm=0;{}\x1b\\",
            &data[..4],
            &data[4..8],
            &data[8..]
        )
        .as_bytes()],
    );
    assert_eq!(layout(&term)[0], [(0, 0, 3, 1)]);
}

#[test]
fn quiet_level_of_a_later_chunk_applies_to_the_whole_transfer() {
    let data = encoded(&COLOR.repeat(2));
    assert_eq!(
        answered(&[format!(
            "\x1b_Ga=t,f=24,s=2,v=1,i=9,m=1;{}\x1b\\\x1b_Gm=0,q=1;{}\x1b\\",
            &data[..4],
            &data[4..]
        )
        .as_bytes()]),
        ""
    );
}

#[test]
fn delete_command_abandons_a_chunked_transfer() {
    let data = encoded(&COLOR.repeat(2));
    assert_eq!(
        answered(&[format!(
            "\x1b_Ga=t,f=24,s=2,v=1,i=9,m=1;{}\x1b\\\x1b_Ga=d\x1b\\\x1b_Gm=0;{}\x1b\\\x1b_Ga=p,i=9\x1b\\",
            &data[..4],
            &data[4..]
        )
        .as_bytes()]),
        answer(9, MISSING)
    );
}

#[test]
fn compressed_pixels_are_inflated() {
    let term = fed(6, 8, &[&kitty("a=T,f=24,s=1,v=1,o=z", &deflated(&COLOR))]);
    assert_eq!(tiles(&term, 0)[0].picture.pixels, [200, 100, 50, 255]);
}

#[test]
fn png_data_carries_its_own_size() {
    let term = fed(6, 8, &[&kitty("a=T,f=100", PNG)]);
    assert_eq!(size(&term), (2, 2));
}

#[test]
fn translucent_pixels_are_stored_premultiplied() {
    let term = fed(6, 8, &[&kitty("a=T,f=100", PNG)]);
    assert_eq!(tiles(&term, 0)[0].picture.pixels, PNG_PIXELS);
}

#[test]
fn compressed_png_data_is_inflated_first() {
    let term = fed(6, 8, &[&kitty("a=T,f=100,o=z", &deflated(PNG))]);
    assert_eq!(tiles(&term, 0)[0].picture.pixels, PNG_PIXELS);
}

#[test]
fn rgba_pixels_are_accepted_by_default() {
    let term = fed(6, 8, &[&kitty("a=T,s=1,v=1", &[255, 0, 0, 128])]);
    assert_eq!(tiles(&term, 0)[0].picture.pixels, [128, 0, 0, 128]);
}

#[test]
fn unsupported_pixel_format_is_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=8,s=1,v=1", &[0])]),
        answer(1, "EINVAL:unsupported image format")
    );
}

#[test]
fn raw_pixels_without_a_size_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=24", &COLOR)]),
        answer(1, "EINVAL:the image size is missing or too large")
    );
}

#[test]
fn raw_pixels_wider_than_the_limit_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,s=4097,v=1", &[0; 4])]),
        answer(1, "EINVAL:the image size is missing or too large")
    );
}

#[test]
fn raw_pixels_taller_than_the_limit_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,s=1,v=4097", &[0; 4])]),
        answer(1, "EINVAL:the image size is missing or too large")
    );
}

#[test]
fn transmission_through_a_file_is_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,t=f,f=100", b"/tmp/image.png")]),
        answer(1, "EINVAL:only direct transmission is supported")
    );
}

#[test]
fn payload_outside_base64_is_rejected() {
    assert_eq!(
        answered(&[b"\x1b_Ga=t,i=1,f=24,s=1,v=1;!!!!\x1b\\"]),
        answer(1, "EINVAL:the payload is not base64")
    );
}

#[test]
fn payload_that_does_not_inflate_is_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=24,s=1,v=1,o=z", &COLOR)]),
        answer(1, "EINVAL:the payload is not zlib data")
    );
}

#[test]
fn rgb_pixels_that_do_not_fill_the_image_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=24,s=2,v=2", &COLOR)]),
        answer(1, "EINVAL:the image data is malformed")
    );
}

#[test]
fn rgba_pixels_that_do_not_fill_the_image_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=32,s=2,v=2", &[0; 4])]),
        answer(1, "EINVAL:the image data is malformed")
    );
}

#[test]
fn broken_png_data_is_rejected() {
    assert_eq!(
        answered(&[&kitty("a=t,i=1,f=100", &PNG[..40])]),
        answer(1, "EINVAL:the image data is malformed")
    );
}

#[test]
fn animation_commands_are_rejected() {
    assert_eq!(
        answered(&[&kitty("a=f,i=1", &[])]),
        answer(1, "EINVAL:unsupported action")
    );
}

#[test]
fn source_rectangle_selects_part_of_the_image() {
    let term = fed(6, 8, &[&checkered("a=T,x=1,y=2,w=2,h=1")]);
    assert_eq!(
        tiles(&term, 0)[0].picture.pixels,
        [10, 20, 0, 255, 20, 20, 0, 255]
    );
}

#[test]
fn source_rectangle_is_clipped_to_the_image() {
    let term = fed(6, 8, &[&checkered("a=T,x=3,y=3,w=100,h=100")]);
    assert_eq!(tiles(&term, 0)[0].picture.pixels, [30, 30, 0, 255]);
}

#[test]
fn source_rectangle_outside_the_image_is_rejected() {
    assert_eq!(
        answered(&[&checkered("a=T,i=1,x=10")]),
        answer(1, "EINVAL:the source rectangle is outside the image")
    );
}

#[test]
fn column_count_scales_the_image_and_keeps_its_shape() {
    let term = fed(6, 8, &[&solid("a=T,c=2", 10, 10)]);
    assert_eq!(size(&term), (20, 20));
}

#[test]
fn row_count_scales_the_image_and_keeps_its_shape() {
    let term = fed(6, 8, &[&solid("a=T,r=2", 10, 10)]);
    assert_eq!(size(&term), (40, 40));
}

#[test]
fn columns_and_rows_letterbox_a_wide_image() {
    let term = fed(6, 8, &[&solid("a=T,c=2,r=2", 20, 10)]);
    let picture = &tiles(&term, 0)[0].picture;
    assert_eq!(
        (
            picture.width,
            picture.height,
            picture.pixels[3],
            picture.pixels[15 * 20 * 4 + 3],
            picture.pixels[25 * 20 * 4 + 3]
        ),
        (20, 40, 0, 255, 0)
    );
}

#[test]
fn columns_and_rows_pillarbox_a_tall_image() {
    let term = fed(6, 8, &[&solid("a=T,c=4,r=2", 10, 40)]);
    let picture = &tiles(&term, 0)[0].picture;
    assert_eq!(
        (
            picture.width,
            picture.height,
            picture.pixels[3],
            picture.pixels[15 * 4 + 3],
            picture.pixels[25 * 4 + 3]
        ),
        (40, 40, 0, 255, 0)
    );
}

#[test]
fn cursor_moves_by_the_requested_columns_and_rows() {
    let term = fed(6, 8, &[&solid("a=T,c=3,r=2", 10, 10)]);
    assert_eq!(cursor(&term), (1, 3));
}

#[test]
fn column_count_past_the_size_limit_is_clamped() {
    let term = fed(6, 8, &[&solid("a=T,C=1,c=1000", 100, 1)]);
    assert_eq!(size(&term), (4090, 40));
}

#[test]
fn row_count_past_the_size_limit_is_clamped() {
    let term = fed(6, 8, &[&solid("a=T,C=1,r=1000", 1, 100)]);
    assert_eq!(size(&term), (40, 4080));
}

#[test]
fn scaled_side_never_drops_below_one_pixel() {
    let term = fed(6, 8, &[&solid("a=T,c=1", 100, 1)]);
    assert_eq!(size(&term), (10, 1));
}

#[test]
fn scaled_side_never_exceeds_the_size_limit() {
    let term = fed(6, 8, &[&solid("a=T,C=1,r=204", 2, 1)]);
    assert_eq!(size(&term), (4096, 4080));
}

#[test]
fn cell_offset_shifts_the_image_inside_its_first_cell() {
    let term = fed(6, 8, &[&solid("a=T,X=3,Y=5", 10, 20)]);
    assert_eq!(
        [&tiles(&term, 0)[0], &tiles(&term, 1)[0]].map(|tile| (
            tile.left,
            tile.top,
            tile.y,
            tile.height
        )),
        [(3, 5, 0, 15), (3, 0, 15, 5)]
    );
}

#[test]
fn cell_offsets_stay_inside_the_cell() {
    let term = fed(6, 8, &[&solid("a=T,X=50,Y=50", 10, 20)]);
    assert_eq!((tiles(&term, 0)[0].left, tiles(&term, 0)[0].top), (9, 19));
}

#[test]
fn tile_reports_the_columns_it_touches() {
    let term = fed(6, 8, &[b"\x1b[1;3H", &solid("a=T,X=3", 10, 20)]);
    assert_eq!(tiles(&term, 0)[0].columns(), 2..4);
}

#[test]
fn tile_records_the_cell_size_it_was_placed_with() {
    let term = fed(6, 8, &[&solid("a=T", 10, 20)]);
    assert_eq!(tiles(&term, 0)[0].cell, [10, 20]);
}

#[test]
fn same_placement_id_moves_the_placement() {
    let term = fed(
        6,
        8,
        &[
            &solid("a=T,i=1,p=2", 10, 20),
            b"\x1b[4;1H",
            &kitty("a=p,i=1,p=2", &[]),
        ],
    );
    assert_eq!(counts(&term), [0, 0, 0, 1, 0, 0]);
}

#[test]
fn placements_without_a_placement_id_accumulate() {
    let term = fed(
        6,
        8,
        &[
            &solid("a=T,i=1", 10, 20),
            b"\x1b[1;1H",
            &kitty("a=p,i=1", &[]),
        ],
    );
    assert_eq!(counts(&term), [2, 0, 0, 0, 0, 0]);
}

#[test]
fn sending_an_image_id_again_removes_its_placements() {
    let term = fed(
        6,
        8,
        &[&solid("a=T,i=1", 10, 20), &solid("a=t,i=1", 10, 20)],
    );
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn unicode_placeholder_placement_is_rejected() {
    assert_eq!(
        answered(&[&solid("a=T,i=1,U=1", 1, 1)]),
        answer(1, "EINVAL:unsupported placement")
    );
}

#[test]
fn relative_placement_is_rejected() {
    assert_eq!(
        answered(&[&solid("a=T,i=1,P=2", 1, 1)]),
        answer(1, "EINVAL:unsupported placement")
    );
}

#[test]
fn delete_without_a_target_removes_every_visible_placement() {
    assert_eq!(deleted("a=d"), [0; 6]);
}

#[test]
fn lowercase_delete_keeps_the_image_data() {
    let mut term = pair();
    feed(&mut term, &kitty("a=d", &[]));
    assert!(kept(&mut term, 1));
}

#[test]
fn uppercase_delete_frees_the_image_data() {
    let mut term = pair();
    feed(&mut term, &kitty("a=d,d=A", &[]));
    assert!(!kept(&mut term, 1));
}

#[test]
fn delete_by_id_removes_that_image_only() {
    assert_eq!(deleted("a=d,d=i,i=1"), [0, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_by_id_without_an_id_removes_nothing() {
    assert_eq!(deleted("a=d,d=i"), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_by_id_and_placement_removes_one_placement() {
    let term = fed(
        6,
        8,
        &[
            &solid("a=T,i=1,p=1", 10, 20),
            b"\x1b[3;1H",
            &kitty("a=p,i=1,p=2", &[]),
            &kitty("a=d,d=i,i=1,p=1", &[]),
        ],
    );
    assert_eq!(counts(&term), [0, 0, 1, 0, 0, 0]);
}

#[test]
fn uppercase_delete_keeps_an_image_that_still_has_a_placement() {
    let mut term = fed(
        6,
        8,
        &[
            &solid("a=T,i=1,p=1", 10, 20),
            b"\x1b[3;1H",
            &kitty("a=p,i=1,p=2", &[]),
            &kitty("a=d,d=I,i=1,p=1", &[]),
        ],
    );
    term.take_reply();
    assert!(kept(&mut term, 1));
}

#[test]
fn uppercase_delete_by_id_frees_an_image_without_placements() {
    let mut term = fed(
        6,
        8,
        &[&solid("a=t,i=1", 10, 20), &kitty("a=d,d=I,i=1", &[])],
    );
    term.take_reply();
    assert!(!kept(&mut term, 1));
}

#[test]
fn delete_by_number_removes_the_newest_image_with_it() {
    let term = fed(
        6,
        8,
        &[&solid("a=T,I=4", 10, 20), &kitty("a=d,d=n,I=4", &[])],
    );
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn delete_by_an_unknown_number_removes_nothing() {
    let term = fed(
        6,
        8,
        &[&solid("a=T,I=4", 10, 20), &kitty("a=d,d=n,I=9", &[])],
    );
    assert_eq!(counts(&term), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_at_the_cursor_removes_the_placement_under_it() {
    let mut term = pair();
    feed(&mut term, b"\x1b[3;4H");
    feed(&mut term, &kitty("a=d,d=c", &[]));
    assert_eq!(counts(&term), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_at_a_cell_removes_the_placement_covering_it() {
    assert_eq!(deleted("a=d,d=p,x=4,y=3"), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_at_a_cell_and_depth_matches_the_only_depth_in_use() {
    assert_eq!(deleted("a=d,d=q,x=4,y=3"), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_at_a_cell_and_another_depth_removes_nothing() {
    assert_eq!(deleted("a=d,d=q,x=4,y=3,z=5"), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_by_column_removes_placements_crossing_it() {
    assert_eq!(deleted("a=d,d=x,x=1"), [0, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_by_row_removes_placements_crossing_it() {
    assert_eq!(deleted("a=d,d=y,y=3"), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_by_depth_removes_placements_at_the_only_depth_in_use() {
    assert_eq!(deleted("a=d,d=z"), [0; 6]);
}

#[test]
fn delete_by_another_depth_removes_nothing() {
    assert_eq!(deleted("a=d,d=z,z=-1"), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_by_id_range_removes_the_images_inside_it() {
    assert_eq!(deleted("a=d,d=r,x=2,y=5"), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn uppercase_delete_by_id_range_frees_only_those_images() {
    let mut term = pair();
    feed(&mut term, &kitty("a=d,d=R,x=2,y=5", &[]));
    assert_eq!((kept(&mut term, 1), kept(&mut term, 2)), (true, false));
}

#[test]
fn delete_of_animation_frames_removes_nothing() {
    assert_eq!(deleted("a=d,d=f"), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn delete_removes_every_row_of_a_placement() {
    let term = fed(6, 8, &[&solid("a=T", 10, 60), &kitty("a=d,d=y,y=2", &[])]);
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn delete_commands_leave_sixel_images() {
    let term = fed(6, 8, &[&sixel("!10~"), &kitty("a=d,d=A", &[])]);
    assert_eq!(counts(&term), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn delete_does_not_reach_into_the_scrollback() {
    let term = fed(
        2,
        8,
        &[&solid("a=T", 10, 20), b"\r\n\n\n", &kitty("a=d", &[])],
    );
    assert_eq!(term.grid().absolute(0).tiles().len(), 1);
}

#[test]
fn erasing_the_whole_screen_removes_images() {
    let mut term = pair();
    feed(&mut term, b"\x1b[2J");
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn erasing_below_the_cursor_keeps_kitty_images() {
    let mut term = pair();
    feed(&mut term, b"\x1b[1;1H\x1b[J");
    assert_eq!(counts(&term), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn erasing_above_the_cursor_keeps_kitty_images() {
    let mut term = pair();
    feed(&mut term, b"\x1b[6;1H\x1b[1J");
    assert_eq!(counts(&term), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn erasing_below_the_cursor_removes_sixel_images() {
    let term = fed(6, 8, &[b"\x1b[3;1H", &sixel("!10~"), b"\x1b[1;1H\x1b[J"]);
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn erasing_part_of_a_line_removes_the_sixel_cells_in_it() {
    let term = fed(6, 8, &[&sixel("!30~"), b"\x1b[1;2H\x1b[K"]);
    assert_eq!(cuts(&term, 0), [(0, 0, 10)]);
}

#[test]
fn text_written_over_a_kitty_image_leaves_it() {
    let mut term = pair();
    feed(&mut term, b"\x1b[1;1Hx");
    assert_eq!(counts(&term), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn text_written_inside_a_sixel_image_splits_it() {
    let term = fed(6, 8, &[&sixel("!30~"), b"\x1b[1;2Hx"]);
    assert_eq!(cuts(&term, 0), [(0, 0, 10), (2, 20, 10)]);
}

#[test]
fn text_written_over_the_left_of_a_sixel_image_keeps_the_rest() {
    let term = fed(6, 8, &[&sixel("!25~"), b"\x1b[1;1Hx"]);
    assert_eq!(cuts(&term, 0), [(1, 10, 15)]);
}

#[test]
fn text_written_over_the_right_of_a_sixel_image_keeps_the_rest() {
    let term = fed(6, 8, &[&sixel("!25~"), b"\x1b[1;3Hx"]);
    assert_eq!(cuts(&term, 0), [(0, 0, 20)]);
}

#[test]
fn text_written_over_all_of_a_sixel_image_removes_it() {
    let term = fed(6, 8, &[&sixel("!30~"), b"\x1b[1;1Hxxx"]);
    assert_eq!(cuts(&term, 0), []);
}

#[test]
fn text_written_beside_a_sixel_image_leaves_it() {
    let term = fed(6, 8, &[&sixel("!30~"), b"\x1b[1;4Hx"]);
    assert_eq!(cuts(&term, 0), [(0, 0, 30)]);
}

#[test]
fn text_written_before_a_sixel_image_leaves_it() {
    let term = fed(6, 8, &[b"\x1b[1;3H", &sixel("!10~"), b"\x1b[1;1Hx"]);
    assert_eq!(cuts(&term, 0), [(2, 0, 10)]);
}

#[test]
fn images_scroll_into_the_history_with_their_rows() {
    let term = fed(2, 8, &[&solid("a=T", 10, 20), b"\r\n\n"]);
    assert_eq!(
        (term.grid().absolute(0).tiles().len(), counts(&term)),
        (1, vec![0, 0])
    );
}

#[test]
fn alternate_screen_starts_without_the_images_of_the_main_screen() {
    let mut term = pair();
    feed(&mut term, b"\x1b[?1049h");
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn main_screen_keeps_its_images_across_the_alternate_screen() {
    let mut term = pair();
    feed(&mut term, b"\x1b[?1049h\x1b[?1049l");
    assert_eq!(counts(&term), [1, 0, 1, 0, 0, 0]);
}

#[test]
fn alternate_screen_is_cleared_of_images_when_it_is_entered_again() {
    let term = fed(
        6,
        8,
        &[
            b"\x1b[?1049h",
            &solid("a=T", 10, 20),
            b"\x1b[?1049l\x1b[?1049h",
        ],
    );
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn full_reset_removes_images_and_their_data() {
    let mut term = pair();
    feed(&mut term, b"\x1bc");
    assert_eq!((counts(&term), kept(&mut term, 1)), (vec![0; 6], false));
}

#[test]
fn full_reset_keeps_the_cell_size() {
    let mut term = pair();
    feed(&mut term, b"\x1bc\x1b[16t");
    assert_eq!(reply(&mut term), "\x1b[6;20;10t");
}

#[test]
fn reflow_keeps_an_image_on_the_row_it_was_placed_on() {
    let mut term = fed(6, 8, &[b"\x1b[2;1H", &solid("a=T,C=1", 10, 20)]);
    term.resize(6, 4);
    assert_eq!(counts(&term), [0, 1, 0, 0, 0, 0]);
}

#[test]
fn reflow_keeps_an_image_on_an_empty_row_below_the_cursor() {
    let mut term = fed(
        6,
        8,
        &[b"\x1b[3;1H", &solid("a=T,C=1", 10, 20), b"\x1b[1;1H"],
    );
    term.resize(6, 4);
    assert_eq!(counts(&term), [0, 0, 1, 0, 0, 0]);
}

#[test]
fn reflow_moves_an_image_with_the_text_of_its_row() {
    let mut term = fed(6, 4, &[b"abcdefgh\x1b[2;1H", &solid("a=T,C=1", 10, 20)]);
    term.resize(6, 2);
    assert_eq!(counts(&term), [0, 0, 1, 0, 0, 0]);
}

#[test]
fn sixel_image_is_tiled_from_the_cursor_row() {
    let term = fed(6, 8, &[&sixel("!10~-!10~-!10~-!10~")]);
    assert_eq!(
        layout(&term)[..3],
        [vec![(0, 0, 10, 20)], vec![(0, 20, 10, 4)], vec![]]
    );
}

#[test]
fn cursor_moves_below_a_sixel_image_and_keeps_its_column() {
    let term = fed(6, 8, &[b"\x1b[1;3H", &sixel("!10~-!10~-!10~-!10~")]);
    assert_eq!(cursor(&term), (2, 2));
}

#[test]
fn sixel_background_is_painted_by_default() {
    let term = fed(6, 8, &[&sixel("\"1;1;2;1@")]);
    let background = term.palette().background;
    assert_eq!(
        tiles(&term, 0)[0].picture.pixels,
        [0, 0, 0, 255, background.r, background.g, background.b, 255]
    );
}

#[test]
fn sixel_background_stays_clear_when_transparency_is_requested() {
    let term = fed(6, 8, &[b"\x1bP0;1q\"1;1;2;1@\x1b\\"]);
    assert_eq!(
        tiles(&term, 0)[0].picture.pixels,
        [0, 0, 0, 255, 0, 0, 0, 0]
    );
}

#[test]
fn sixel_image_replaces_an_earlier_one_in_the_same_cells() {
    let term = fed(6, 8, &[&sixel("!10~"), b"\x1b[1;1H", &sixel("!10~")]);
    assert_eq!(
        tiles(&term, 0)
            .iter()
            .map(|tile| tile.picture.serial)
            .collect::<Vec<u64>>(),
        [2]
    );
}

#[test]
fn sixel_data_without_pixels_places_nothing() {
    let term = fed(6, 8, &[&sixel("")]);
    assert_eq!((counts(&term), cursor(&term)), (vec![0; 6], (0, 0)));
}

#[test]
fn device_control_string_with_an_intermediate_is_not_sixel() {
    let term = fed(6, 8, &[b"\x1bP$q~\x1b\\"]);
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn device_control_string_with_another_final_byte_is_not_sixel() {
    let term = fed(6, 8, &[b"\x1bPp~\x1b\\"]);
    assert_eq!(counts(&term), [0; 6]);
}

#[test]
fn device_control_string_with_too_many_parameters_is_not_sixel() {
    let term = fed(
        6,
        8,
        &[format!("\x1bP{}q~\x1b\\", "1;".repeat(40)).as_bytes()],
    );
    assert_eq!(counts(&term), [0; 6]);
}
