//! ACE's `Source/ACE.Entity/Enum` enums, one module per C# file, re-exported flat.
//!
//! Each is a transparent newtype over its underlying integer, with one associated constant
//! per member; `support.rs` has the shared .NET `System.Enum` semantics.
// @generated from ACE's `Source/ACE.Entity/Enum` folder; do not edit by hand

pub mod ext;
pub mod properties;
mod support;

pub use properties::*;
pub use support::{get_name, AceEnum};

#[rustfmt::skip]
mod access_level;
#[rustfmt::skip]
mod activation_response;
#[rustfmt::skip]
mod aetheria_bitfield;
#[rustfmt::skip]
mod ai_option;
#[rustfmt::skip]
mod allegiance_house_action;
#[rustfmt::skip]
mod allegiance_lock_action;
#[rustfmt::skip]
mod allegiance_officer_level;
#[rustfmt::skip]
mod allegiance_permission_level;
#[rustfmt::skip]
mod ammo_type;
#[rustfmt::skip]
mod animation_flags;
#[rustfmt::skip]
mod animation_hook_dir;
#[rustfmt::skip]
mod animation_hook_type;
#[rustfmt::skip]
mod appraisal_long_desc_decorations;
#[rustfmt::skip]
mod armor_type;
#[rustfmt::skip]
mod attack_conditions;
#[rustfmt::skip]
mod attack_height;
#[rustfmt::skip]
mod attack_type;
#[rustfmt::skip]
mod attribute_cache;
#[rustfmt::skip]
mod attuned_status;
#[rustfmt::skip]
mod augmentation_type;
#[rustfmt::skip]
mod auth_flags;
#[rustfmt::skip]
mod base_property_type;
#[rustfmt::skip]
mod bonded_status;
#[rustfmt::skip]
mod bsp_type;
#[rustfmt::skip]
mod channel;
#[rustfmt::skip]
mod character_option;
#[rustfmt::skip]
mod character_option_data_flag;
#[rustfmt::skip]
mod character_options1;
#[rustfmt::skip]
mod character_options2;
#[rustfmt::skip]
mod character_title;
#[rustfmt::skip]
mod chat_display_mask;
#[rustfmt::skip]
mod chat_filter_mask;
#[rustfmt::skip]
mod chat_message_type;
#[rustfmt::skip]
mod chat_network_blob_dispatch_type;
#[rustfmt::skip]
mod chat_network_blob_type;
#[rustfmt::skip]
mod chat_type;
#[rustfmt::skip]
mod chess_ai_state;
#[rustfmt::skip]
mod chess_color;
#[rustfmt::skip]
mod chess_delayed_action_type;
#[rustfmt::skip]
mod chess_move_flag;
#[rustfmt::skip]
mod chess_move_result;
#[rustfmt::skip]
mod chess_move_type;
#[rustfmt::skip]
mod chess_piece_type;
#[rustfmt::skip]
mod chess_state;
#[rustfmt::skip]
mod cloak_status;
#[rustfmt::skip]
mod combat_body_part;
#[rustfmt::skip]
mod combat_mode;
#[rustfmt::skip]
mod combat_style;
#[rustfmt::skip]
mod combat_use;
#[rustfmt::skip]
mod command_masks;
#[rustfmt::skip]
mod compare_type;
#[rustfmt::skip]
mod confirmation_type;
#[rustfmt::skip]
mod container_type;
#[rustfmt::skip]
mod content_type;
#[rustfmt::skip]
mod contract_id;
#[rustfmt::skip]
mod coverage_mask;
#[rustfmt::skip]
mod creature_type;
#[rustfmt::skip]
mod cull_mode;
#[rustfmt::skip]
mod damage_type;
#[rustfmt::skip]
mod database_selection_option;
#[rustfmt::skip]
mod delayed_action_type;
#[rustfmt::skip]
mod destination_type;
#[rustfmt::skip]
mod dispel_type;
#[rustfmt::skip]
mod effect_argument_type;
#[rustfmt::skip]
mod emitter_type;
#[rustfmt::skip]
mod emote_category;
#[rustfmt::skip]
mod emote_type;
#[rustfmt::skip]
mod enchantment_category;
#[rustfmt::skip]
mod enchantment_type_flags;
#[rustfmt::skip]
mod end_trade_reason;
#[rustfmt::skip]
mod env_cell_flags;
#[rustfmt::skip]
mod environ_change_type;
#[rustfmt::skip]
mod equip_mask;
#[rustfmt::skip]
mod equipment_set;
#[rustfmt::skip]
mod experience_handling_type;
#[rustfmt::skip]
mod faction_bits;
#[rustfmt::skip]
mod fellow_update_type;
#[rustfmt::skip]
mod game_event_state;
#[rustfmt::skip]
mod game_piece_state;
#[rustfmt::skip]
mod gender;
#[rustfmt::skip]
mod generator_defined_times;
#[rustfmt::skip]
mod generator_destruct;
#[rustfmt::skip]
mod generator_time_type;
#[rustfmt::skip]
mod generator_type;
#[rustfmt::skip]
mod gfx_obj_flags;
#[rustfmt::skip]
mod har_bitfield;
#[rustfmt::skip]
mod heritage_group;
#[rustfmt::skip]
mod hold_key;
#[rustfmt::skip]
mod hook_group_type;
#[rustfmt::skip]
mod hook_type;
#[rustfmt::skip]
mod house_bitfield;
#[rustfmt::skip]
mod house_status;
#[rustfmt::skip]
mod house_type;
#[rustfmt::skip]
mod identify_response_flags;
#[rustfmt::skip]
mod id_lookup_type;
#[rustfmt::skip]
mod image_scale_type;
#[rustfmt::skip]
mod imbued_effect_type;
#[rustfmt::skip]
mod incorporation_flags;
#[rustfmt::skip]
mod item_type;
#[rustfmt::skip]
mod item_xp_style;
#[rustfmt::skip]
mod lifestone_type;
#[rustfmt::skip]
mod magic_school;
#[rustfmt::skip]
mod map_scope;
#[rustfmt::skip]
mod material_type;
#[rustfmt::skip]
mod media_type;
#[rustfmt::skip]
mod modification_operation;
#[rustfmt::skip]
mod modification_type;
#[rustfmt::skip]
mod modifier_type;
#[rustfmt::skip]
mod motion_command;
#[rustfmt::skip]
mod motion_data_flags;
#[rustfmt::skip]
mod motion_flags;
#[rustfmt::skip]
mod motion_stance;
#[rustfmt::skip]
mod movement_command;
#[rustfmt::skip]
mod movement_option;
#[rustfmt::skip]
mod movement_params;
#[rustfmt::skip]
mod movement_state_flag;
#[rustfmt::skip]
mod movement_type;
#[rustfmt::skip]
mod movement_types;
#[rustfmt::skip]
mod mutate_filter;
#[rustfmt::skip]
mod mutation_effect_type;
#[rustfmt::skip]
mod numbering_type;
#[rustfmt::skip]
mod object_description_flag;
#[rustfmt::skip]
mod palette_template;
#[rustfmt::skip]
mod parent_location;
#[rustfmt::skip]
mod particle_type;
#[rustfmt::skip]
mod physics_description_flag;
#[rustfmt::skip]
mod physics_state;
#[rustfmt::skip]
mod pickup_state;
#[rustfmt::skip]
mod pk_level;
#[rustfmt::skip]
mod placement;
#[rustfmt::skip]
mod player_killer_status;
#[rustfmt::skip]
mod play_script;
#[rustfmt::skip]
mod portal_bitmask;
#[rustfmt::skip]
mod portal_flags;
#[rustfmt::skip]
mod portal_link_type;
#[rustfmt::skip]
mod portal_recall_type;
#[rustfmt::skip]
mod portal_summon_type;
#[rustfmt::skip]
mod portal_type;
#[rustfmt::skip]
mod position_flags;
#[rustfmt::skip]
mod power_accuracy;
#[rustfmt::skip]
mod property_caching_type;
#[rustfmt::skip]
mod property_dat_file_type;
#[rustfmt::skip]
mod property_group_name;
#[rustfmt::skip]
mod property_inheritance_type;
#[rustfmt::skip]
mod property_propagation_type;
#[rustfmt::skip]
mod quadrant;
#[rustfmt::skip]
mod quadrant_index;
#[rustfmt::skip]
mod radar_behavior;
#[rustfmt::skip]
mod radar_color;
#[rustfmt::skip]
mod recipe_result;
#[rustfmt::skip]
mod recipe_source_type;
#[rustfmt::skip]
mod recipe_type;
#[rustfmt::skip]
mod regeneration_type;
#[rustfmt::skip]
mod regen_location_type;
#[rustfmt::skip]
mod requirement_type;
#[rustfmt::skip]
mod resistance_type;
#[rustfmt::skip]
mod security_level;
#[rustfmt::skip]
mod setup_const;
#[rustfmt::skip]
mod setup_flags;
#[rustfmt::skip]
mod share_type;
#[rustfmt::skip]
mod sidedness;
#[rustfmt::skip]
mod simple_polygon_type;
#[rustfmt::skip]
mod skill;
#[rustfmt::skip]
mod skill_advancement_class;
#[rustfmt::skip]
mod sound;
#[rustfmt::skip]
mod source_selection_option;
#[rustfmt::skip]
mod spell_bitfield;
#[rustfmt::skip]
mod spell_book_filter_options;
#[rustfmt::skip]
mod spell_category;
#[rustfmt::skip]
mod spell_flags;
#[rustfmt::skip]
mod spell_id;
#[rustfmt::skip]
mod spell_type;
#[rustfmt::skip]
mod squelch_mask;
#[rustfmt::skip]
mod stance_mode;
#[rustfmt::skip]
mod stat_type;
#[rustfmt::skip]
mod stippling_type;
#[rustfmt::skip]
mod subscription_status;
#[rustfmt::skip]
mod summoning_mastery;
#[rustfmt::skip]
mod surface_handler;
#[rustfmt::skip]
mod surface_pixel_format;
#[rustfmt::skip]
mod surface_type;
#[rustfmt::skip]
mod targeting_tactic;
#[rustfmt::skip]
mod texture_type;
#[rustfmt::skip]
mod toggle_type;
#[rustfmt::skip]
mod tolerance;
#[rustfmt::skip]
mod trade_side;
#[rustfmt::skip]
mod transfer_flags;
#[rustfmt::skip]
mod treasure_class;
#[rustfmt::skip]
mod treasure_type;
#[rustfmt::skip]
mod ui_effects;
#[rustfmt::skip]
mod update_position_flag;
#[rustfmt::skip]
mod usable;
#[rustfmt::skip]
mod vendor_type;
#[rustfmt::skip]
mod vertex_type;
#[rustfmt::skip]
mod vital;
#[rustfmt::skip]
mod weapon_type;
#[rustfmt::skip]
mod weenie_class_name;
#[rustfmt::skip]
mod weenie_error;
#[rustfmt::skip]
mod weenie_error_with_string;
#[rustfmt::skip]
mod weenie_header_flags;
#[rustfmt::skip]
mod weenie_type;
#[rustfmt::skip]
mod wield_requirement;
#[rustfmt::skip]
mod xp_type;

pub use access_level::AccessLevel;
pub use activation_response::ActivationResponse;
pub use aetheria_bitfield::AetheriaBitfield;
pub use ai_option::AiOption;
pub use allegiance_house_action::AllegianceHouseAction;
pub use allegiance_lock_action::AllegianceLockAction;
pub use allegiance_officer_level::AllegianceOfficerLevel;
pub use allegiance_permission_level::AllegiancePermissionLevel;
pub use ammo_type::AmmoType;
pub use animation_flags::AnimationFlags;
pub use animation_hook_dir::AnimationHookDir;
pub use animation_hook_type::AnimationHookType;
pub use appraisal_long_desc_decorations::AppraisalLongDescDecorations;
pub use armor_type::ArmorType;
pub use attack_conditions::AttackConditions;
pub use attack_height::AttackHeight;
pub use attack_type::AttackType;
pub use attribute_cache::AttributeCache;
pub use attuned_status::AttunedStatus;
pub use augmentation_type::AugmentationType;
pub use auth_flags::AuthFlags;
pub use base_property_type::BasePropertyType;
pub use bonded_status::BondedStatus;
pub use bsp_type::BSPType;
pub use channel::Channel;
pub use character_option::CharacterOption;
pub use character_option_data_flag::CharacterOptionDataFlag;
pub use character_options1::CharacterOptions1;
pub use character_options2::CharacterOptions2;
pub use character_title::CharacterTitle;
pub use chat_display_mask::ChatDisplayMask;
pub use chat_filter_mask::ChatFilterMask;
pub use chat_message_type::ChatMessageType;
pub use chat_network_blob_dispatch_type::ChatNetworkBlobDispatchType;
pub use chat_network_blob_type::ChatNetworkBlobType;
pub use chat_type::ChatType;
pub use chess_ai_state::ChessAiState;
pub use chess_color::ChessColor;
pub use chess_delayed_action_type::ChessDelayedActionType;
pub use chess_move_flag::ChessMoveFlag;
pub use chess_move_result::ChessMoveResult;
pub use chess_move_type::ChessMoveType;
pub use chess_piece_type::ChessPieceType;
pub use chess_state::ChessState;
pub use cloak_status::CloakStatus;
pub use combat_body_part::CombatBodyPart;
pub use combat_mode::CombatMode;
pub use combat_style::CombatStyle;
pub use combat_use::CombatUse;
pub use command_masks::CommandMask;
pub use compare_type::CompareType;
pub use confirmation_type::ConfirmationType;
pub use container_type::ContainerType;
pub use content_type::ContentType;
pub use contract_id::ContractId;
pub use coverage_mask::{CoverageMask, CoverageMaskHelper};
pub use creature_type::CreatureType;
pub use cull_mode::CullMode;
pub use damage_type::DamageType;
pub use database_selection_option::DatabaseSelectionOption;
pub use delayed_action_type::DelayedActionType;
pub use destination_type::DestinationType;
pub use dispel_type::DispelType;
pub use effect_argument_type::EffectArgumentType;
pub use emitter_type::EmitterType;
pub use emote_category::EmoteCategory;
pub use emote_type::EmoteType;
pub use enchantment_category::EnchantmentMask;
pub use enchantment_type_flags::EnchantmentTypeFlags;
pub use end_trade_reason::EndTradeReason;
pub use env_cell_flags::EnvCellFlags;
pub use environ_change_type::EnvironChangeType;
pub use equip_mask::EquipMask;
pub use equipment_set::EquipmentSet;
pub use experience_handling_type::ExperienceHandlingType;
pub use faction_bits::FactionBits;
pub use fellow_update_type::FellowUpdateType;
pub use game_event_state::GameEventState;
pub use game_piece_state::GamePieceState;
pub use gender::Gender;
pub use generator_defined_times::GeneratorDefinedTimes;
pub use generator_destruct::GeneratorDestruct;
pub use generator_time_type::GeneratorTimeType;
pub use generator_type::GeneratorType;
pub use gfx_obj_flags::GfxObjFlags;
pub use har_bitfield::HARBitfield;
pub use heritage_group::HeritageGroup;
pub use hold_key::HoldKey;
pub use hook_group_type::HookGroupType;
pub use hook_type::HookType;
pub use house_bitfield::HouseBitfield;
pub use house_status::HouseStatus;
pub use house_type::HouseType;
pub use id_lookup_type::AccountLookupType;
pub use identify_response_flags::IdentifyResponseFlags;
pub use image_scale_type::ImageScaleType;
pub use imbued_effect_type::ImbuedEffectType;
pub use incorporation_flags::IncorporationFlags;
pub use item_type::ItemType;
pub use item_xp_style::ItemXpStyle;
pub use lifestone_type::LifestoneType;
pub use magic_school::MagicSchool;
pub use map_scope::MapScope;
pub use material_type::MaterialType;
pub use media_type::MediaType;
pub use modification_operation::ModificationOperation;
pub use modification_type::ModificationType;
pub use modifier_type::ModifierType;
pub use motion_command::MotionCommand;
pub use motion_data_flags::MotionDataFlags;
pub use motion_flags::MotionFlags;
pub use motion_stance::MotionStance;
pub use movement_command::MovementCommand;
pub use movement_option::MovementOption;
pub use movement_params::MovementParams;
pub use movement_state_flag::MovementStateFlag;
pub use movement_type::MovementType;
pub use movement_types::MovementTypes;
pub use mutate_filter::MutateFilter;
pub use mutation_effect_type::MutationEffectType;
pub use numbering_type::NumberingType;
pub use object_description_flag::ObjectDescriptionFlag;
pub use palette_template::PaletteTemplate;
pub use parent_location::ParentLocation;
pub use particle_type::ParticleType;
pub use physics_description_flag::PhysicsDescriptionFlag;
pub use physics_state::PhysicsState;
pub use pickup_state::PickupState;
pub use pk_level::PKLevel;
pub use placement::Placement;
pub use play_script::PlayScript;
pub use player_killer_status::PlayerKillerStatus;
pub use portal_bitmask::PortalBitmask;
pub use portal_flags::PortalFlags;
pub use portal_link_type::PortalLinkType;
pub use portal_recall_type::PortalRecallType;
pub use portal_summon_type::PortalSummonType;
pub use portal_type::PortalType;
pub use position_flags::PositionFlags;
pub use power_accuracy::PowerAccuracy;
pub use property_caching_type::PropertyCachingType;
pub use property_dat_file_type::PropertyDatFileType;
pub use property_group_name::PropertyGroupName;
pub use property_inheritance_type::PropertyInheritanceType;
pub use property_propagation_type::PropertyPropagationType;
pub use quadrant::Quadrant;
pub use quadrant_index::QuadrantIndex;
pub use radar_behavior::RadarBehavior;
pub use radar_color::RadarColor;
pub use recipe_result::RecipeResult;
pub use recipe_source_type::RecipeSourceType;
pub use recipe_type::RecipeType;
pub use regen_location_type::RegenLocationType;
pub use regeneration_type::RegenerationType;
pub use requirement_type::RequirementType;
pub use resistance_type::ResistanceType;
pub use security_level::SecurityLevel;
pub use setup_const::SetupConst;
pub use setup_flags::SetupFlags;
pub use share_type::ShareType;
pub use sidedness::Sidedness;
pub use simple_polygon_type::SimplePolygonType;
pub use skill::Skill;
pub use skill_advancement_class::SkillAdvancementClass;
pub use sound::Sound;
pub use source_selection_option::SourceSelectionOption;
pub use spell_bitfield::SpellBitfield;
pub use spell_book_filter_options::SpellBookFilterOptions;
pub use spell_category::SpellCategory;
pub use spell_flags::SpellFlags;
pub use spell_id::SpellId;
pub use spell_type::SpellType;
pub use squelch_mask::SquelchMask;
pub use stance_mode::StanceMode;
pub use stat_type::StatType;
pub use stippling_type::StipplingType;
pub use subscription_status::SubscriptionStatus;
pub use summoning_mastery::SummoningMastery;
pub use surface_handler::SurfaceHandler;
pub use surface_pixel_format::SurfacePixelFormat;
pub use surface_type::SurfaceType;
pub use targeting_tactic::TargetingTactic;
pub use texture_type::TextureType;
pub use toggle_type::ToggleType;
pub use tolerance::Tolerance;
pub use trade_side::TradeSide;
pub use transfer_flags::TransferFlags;
pub use treasure_class::TreasureClass;
pub use treasure_type::TreasureType;
pub use ui_effects::UiEffects;
pub use update_position_flag::UpdatePositionFlag;
pub use usable::Usable;
pub use vendor_type::VendorType;
pub use vertex_type::VertexType;
pub use vital::Vital;
pub use weapon_type::WeaponType;
pub use weenie_class_name::WeenieClassName;
pub use weenie_error::WeenieError;
pub use weenie_error_with_string::WeenieErrorWithString;
pub use weenie_header_flags::{WeenieHeaderFlag, WeenieHeaderFlag2};
pub use weenie_type::WeenieType;
pub use wield_requirement::WieldRequirement;
pub use xp_type::XpType;
