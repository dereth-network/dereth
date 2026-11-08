//! What `@clouds` has asked for, laid over the knobs the sky is drawn with. The bridge reads the
//! result; the copy it changes is its own.

use dereth_client_runtime::clouds::{CloudOption, Clouds, OPTIONS as CLOUD_OPTIONS};
use dereth_terrain::land::drawn::{Knob, Knobs};

/// The knob each `@clouds` option sets.
const fn cloud_knob(option: CloudOption) -> Knob {
    match option {
        CloudOption::Cover => Knob::CloudCover,
        CloudOption::Speed => Knob::CloudSpeed,
        CloudOption::Height => Knob::CloudHeight,
        CloudOption::Depth => Knob::CloudDepth,
        CloudOption::Size => Knob::CloudScale,
        CloudOption::Detail => Knob::CloudDetail,
        CloudOption::Light => Knob::CloudLight,
        CloudOption::Shade => Knob::CloudShade,
        CloudOption::Shadows => Knob::CloudShadows,
    }
}

/// The knobs with what `@clouds` has asked for in place of their own values.
pub(super) fn with_asked_clouds(k: &Knobs, asked: &Clouds) -> Knobs {
    let mut k = k.clone();
    for option in CLOUD_OPTIONS {
        if let Some(value) = asked.get(option) {
            k.set(cloud_knob(option), value);
        }
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::hifi_bridge::sky_knobs;

    /// Behaviour: rendering.sky.clouds-asked-for-replace-the-settings-until-reset
    #[test]
    fn the_clouds_asked_for_replace_the_knobs_and_reset_returns_to_them() {
        use dereth_client_runtime::clouds::CloudsCommand as C;
        let mut own = Knobs::default();
        own.set(Knob::CloudLight, 2.5);
        let mut asked = Clouds::default();
        assert_eq!(with_asked_clouds(&own, &asked), own, "nothing asked for");

        asked.obey(C::Set(CloudOption::Cover, 0.0));
        asked.obey(C::Set(CloudOption::Size, 3.0));
        let sky = sky_knobs(&with_asked_clouds(&own, &asked)).expect("the sky's knob is on");
        assert_eq!(sky.cloud_cover, 0.0);
        assert_eq!(sky.cloud_scale, 3.0);
        assert_eq!(
            sky.cloud_light, 2.5,
            "what is not asked for stays the knob's"
        );

        asked.obey(C::Reset);
        assert_eq!(with_asked_clouds(&own, &asked), own);
    }

    /// Behaviour: rendering.sky.clouds-asked-for-replace-the-settings-until-reset
    #[test]
    fn each_cloud_option_sets_its_own_knob_and_takes_that_knobs_whole_range() {
        let mut knobs = std::collections::HashSet::new();
        for option in CLOUD_OPTIONS {
            let knob = cloud_knob(option);
            let spec = knob.spec();
            assert_eq!(option.range(), (spec.min, spec.max), "{option:?}");
            assert!(knobs.insert(knob), "{option:?} shares a knob");
        }
    }
}
