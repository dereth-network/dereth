//! ACE: Source/ACE.Server/WorldObjects/AugmentationDevice.cs::AugmentationDevice
//! Augmentation devices through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod devices {
    //! ACE: Source/ACE.Server/WorldObjects/AugmentationDevice.cs::AugmentationDevice
    use crate::support::tracking_world::*;

    /// An augmentation gem (Strength): the use asks "This action will augment your character with
    /// ... and will cost 1,000 available experience." (`Confirmation_Augmentation`); the yes over the
    /// wire applies it (`DoAugmentation`): innate Strength +5 (`Math.Min(5, 100 - 50)`: the safety
    /// cap is on by default), the augmentation property and the innate family counter, 1,000
    /// experience spent, the gem consumed, the success string, the attribute effect and the broadcast
    /// line.
    #[test]
    fn an_augmentation_gem_used_and_confirmed_applies_the_augmentation() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let gem = in_pack(&mut ts.world, ALPHA, STRENGTH_GEM);
        ts.advance(0.1);

        let (ask, g) = use_and_confirm(&mut ts, a, gem);
        assert_eq!(
            ask.confirmation_type,
            i32::try_from(ConfirmationType::Augmentation.0).unwrap()
        );
        assert_eq!(ask.text, "This action will augment your character with Might of the Seventh Mule and will cost 1,000 available experience.");

        let attr: QualitiesPrivateUpdateAttribute = one(&g, PRIVATE_ATTRIBUTE);
        assert_eq!(
            (attr.0.property_id, attr.0.value.init_level),
            (u32::from(PropertyAttribute::Strength.0), 55)
        );
        let ints: Vec<QualitiesPrivateUpdateInt> = g
            .iter()
            .filter(|(k, _)| *k == PRIVATE_INT)
            .map(|(_, b)| decode(b))
            .collect();
        assert!(
            ints.iter().any(|i| i.0.property_id
                == u32::from(PropertyInt::AugmentationInnateStrength.0)
                && i.0.value == 1),
            "AugmentationInnateStrength 1: {ints:?}"
        );
        let xp: QualitiesPrivateUpdateInt64 = one(&g, PRIVATE_INT64);
        assert_eq!(
            (xp.0.property_id, xp.0.value),
            (u32::from(PropertyInt64::AvailableExperience.0), 4000)
        );
        let done: CommunicationWeenieErrorWithString = one(&g, ERROR_WITH_STRING);
        assert_eq!(
            done.error_type,
            WeenieErrorWithString::YouSuccededAcquiringAugmentation
                .0
                .cast_unsigned()
        );
        assert_eq!(done.text, "Might of the Seventh Mule");
        let effect: EffectsPlayScriptType = one(&g, PLAY_EFFECT);
        assert_eq!(effect.id.0, ALPHA);
        let chat: CommunicationTextboxString = one(&g, CHAT);
        assert_eq!(
            chat.text,
            "Alpha has acquired the Might of the Seventh Mule augmentation!"
        );

        let w = &ts.world;
        let o = w.objects.get(ObjectGuid::new(ALPHA)).unwrap();
        assert_eq!(innate(w, ALPHA, PropertyAttribute::Strength), 55);
        assert_eq!(
            o.get_property(PropertyInt::AugmentationInnateStrength),
            Some(1)
        );
        assert_eq!(o.augmentation_innate_family(), 1);
        assert_eq!(o.available_experience(), Some(4000));
        assert!(
            !container::inventory_values(w, ObjectGuid::new(ALPHA)).contains(&gem),
            "the gem is consumed"
        );
    }

    /// An attribute transfer device (Endurance to Strength): asked with `Confirmation_AlterAttribute`,
    /// the yes moves 10 innate points and updates both attributes, then consumes the device.
    #[test]
    fn an_attribute_transfer_device_confirmed_moves_ten_points() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let device = in_pack(&mut ts.world, ALPHA, TRANSFER_DEVICE);
        ts.advance(0.1);

        let (ask, g) = use_and_confirm(&mut ts, a, device);
        assert_eq!(
            ask.confirmation_type,
            i32::try_from(ConfirmationType::AlterAttribute.0).unwrap()
        );
        assert_eq!(
            ask.text,
            "This action will transfer 10 points from your Endurance to your Strength."
        );
        let attrs: Vec<QualitiesPrivateUpdateAttribute> = g
            .iter()
            .filter(|(k, _)| *k == PRIVATE_ATTRIBUTE)
            .map(|(_, b)| decode(b))
            .collect();
        let got: Vec<(u32, u32)> = attrs
            .iter()
            .map(|u| (u.0.property_id, u.0.value.init_level))
            .collect();
        assert_eq!(
            got,
            [
                (u32::from(PropertyAttribute::Endurance.0), 50),
                (u32::from(PropertyAttribute::Strength.0), 60)
            ]
        );
        assert_eq!(innate(&ts.world, ALPHA, PropertyAttribute::Endurance), 50);
        assert_eq!(innate(&ts.world, ALPHA, PropertyAttribute::Strength), 60);
        assert!(
            !container::inventory_values(&ts.world, ObjectGuid::new(ALPHA)).contains(&device),
            "the device is consumed"
        );
    }

    /// V342 (a fix): with the attribute the device raises (Strength) already at 100, the refusal
    /// names Strength. ACE named the attribute the device lowers (Endurance).
    #[test]
    fn an_attribute_transfer_device_refusal_names_the_attribute_at_its_maximum() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let device = in_pack(&mut ts.world, ALPHA, TRANSFER_DEVICE);
        {
            let o = ts.world.objects.get_mut(ObjectGuid::new(ALPHA)).unwrap();
            let strength = *o.attributes().get(&PropertyAttribute::Strength).unwrap();
            strength.set_starting_value(o, 100);
        }
        ts.advance(0.1);

        let n = ts.received_raw(a).len();
        empyrean_world::dispatch::act_on_use::act_on_use(
            &mut ts.world,
            device,
            ObjectGuid::new(ALPHA),
        );
        ts.advance(0.2);
        let g = events(got(&ts, a, n));
        let refused: CommunicationWeenieErrorWithString = one(&g, ERROR_WITH_STRING);
        assert_eq!(
            refused.error_type,
            WeenieErrorWithString::AttributeTransferToTooHigh
                .0
                .cast_unsigned()
        );
        assert_eq!(refused.text, "Your innate level of Strength is already as high as it can be. You may not increase it any further.");
        assert!(!g.iter().any(|(k, _)| *k == CONFIRM), "nothing is asked");
    }
}
