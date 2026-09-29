//! The media-image setter the chess board uses: setting an element's media image replaces its media
//! and draws with the native draw-mode numbers, and setting it for one state replaces that state's
//! media and repaints only when the state is current. Fixture: a `UiSystem` with one element built
//! in the test; no dats.
//!
//! Behaviour: none (the media setter's contract, below any player-visible claim)

use dereth_primitives::DataId;
use dereth_ui::desc::{MediaDesc, MediaFields, StateDesc};
use dereth_ui::{BlitMode, GraphicRef, MediaEffect, StateId, UiSystem};

#[test]
fn set_media_image_replaces_media_and_uses_the_native_draw_mode_numbers() {
    let mut ui = UiSystem::new((800, 600));
    let h = ui.create_hollow(Some(ui.root()));

    // An image-update effect reaches the same draw-mode switch as setting the media image.
    ui.apply_media_effect(
        h,
        MediaEffect::SetImage {
            file: Some(DataId(0x0600_0001)),
            draw_mode: 2,
        },
    );
    assert_eq!(
        ui.node(h).expect("element").region.blit_mode,
        BlitMode::Alpha3
    );

    // Model a media machine waiting on global tick 3: `set_media_image`'s cleanup must remove it.
    ui.node_mut(h).expect("element").media.registered_for_tick = true;
    ui.set_media_image(h, DataId(0x0600_4D06), 3);
    let node = ui.node(h).expect("element");
    assert_eq!(
        node.region.image,
        Some(GraphicRef::opaque_surface(DataId(0x0600_4D06), 0, 0))
    );
    assert_eq!(node.region.blit_mode, BlitMode::Alpha4);
    assert!(
        !node.media.registered_for_tick,
        "Cleanup removed the old tick registration"
    );

    ui.set_media_image(h, DataId(0), 17);
    let node = ui.node(h).expect("element");
    assert_eq!(
        node.region.image, None,
        "INVALID_DID clears the prior image"
    );
    assert_eq!(
        node.region.blit_mode,
        BlitMode::Normal,
        "all other draw modes are normal"
    );
}

#[test]
fn set_media_image_for_state_replaces_the_state_and_only_updates_it_when_current() {
    let mut ui = UiSystem::new((800, 600));
    let h = ui.create_hollow(Some(ui.root()));
    let state = StateId(6);
    ui.node_mut(h).expect("element").desc.states.insert(
        state,
        StateDesc {
            state_id: state,
            media: vec![
                MediaDesc {
                    media_type: 8,
                    type_echo_ok: true,
                    fields: MediaFields::Pause {
                        min_duration: 1.0,
                        max_duration: 2.0,
                    },
                },
                MediaDesc {
                    media_type: 5,
                    type_echo_ok: true,
                    fields: MediaFields::Image {
                        file: DataId(0x0600_0001),
                        draw_mode: 2,
                    },
                },
            ],
            ..StateDesc::default()
        },
    );
    let old = DataId(0x0600_4D06);
    let first = DataId(0x0600_4D17);
    let second = DataId(0x0600_4D19);
    ui.set_media_image(h, old, 3);

    // The alternate state is mutated immediately, but a non-current state cannot change pixels.
    ui.set_media_image_for_state(h, first, 1, state);
    assert_eq!(
        ui.node(h).expect("element").region.image,
        Some(GraphicRef::opaque_surface(old, 0, 0))
    );
    let media = &ui.node(h).expect("element").desc.states[&state].media;
    assert_eq!(
        media.len(),
        1,
        "setting a state's image replaces its previous media list"
    );
    assert!(matches!(
        media[0].fields,
        MediaFields::Image { file, draw_mode: 1 } if file == first
    ));

    // Entering that state later runs the newly installed image.
    ui.set_state(h, state);
    assert_eq!(
        ui.node(h).expect("element").region.image,
        Some(GraphicRef::opaque_surface(first, 0, 0))
    );

    // The same call on the current state follows retail's immediate media-image tail.
    ui.set_media_image_for_state(h, second, 1, state);
    assert_eq!(
        ui.node(h).expect("element").region.image,
        Some(GraphicRef::opaque_surface(second, 0, 0))
    );
}
