//! A synthetic mysqldump world database covering all 54 ACE world tables and edge-case rows.
//! Fixture: synthetic world records, SQL dumps and JSON documents.

use std::sync::OnceLock;

use empyrean_content::import::{import, Imported};
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;

/// `bit(1)` true, as mysqldump writes it: a raw 0x01 byte inside quotes.
const T: &str = "'\u{1}'";
/// `bit(1)` false: the `\0` escape.
const F: &str = "'\\0'";

/// The dump text.
#[must_use]
pub fn dump() -> String {
    let mut s = String::new();
    s.push_str(HEADER);
    s.push_str(SCHEMA_A);
    s.push_str(SCHEMA_B);
    s.push_str(&rows().replace("{T}", T).replace("{F}", F));
    s
}

const HEADER: &str = "-- MySQL dump 10.13  Distrib 5.7.17, for Win64 (x86_64)\n\
--\n\
-- Host: localhost    Database: ace_world\n\
/*!40101 SET NAMES utf8 */;\n\
USE `ace_world`;\n\
DROP TABLE IF EXISTS `cook_book`;\n";

const SCHEMA_A: &str = r"CREATE TABLE `cook_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this cook book instance',
  `recipe_Id` int(10) unsigned NOT NULL,
  `source_W_C_I_D` int(10) unsigned NOT NULL,
  `target_W_C_I_D` int(10) unsigned NOT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `recipe_source_target_uidx` (`recipe_Id`,`source_W_C_I_D`,`target_W_C_I_D`),
  CONSTRAINT `cookbook_recipe` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=55930 DEFAULT CHARSET=utf8 COMMENT='Cook Book for Recipes';
CREATE TABLE `encounter` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `landblock` int(5) NOT NULL,
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `cell_X` int(5) NOT NULL,
  `cell_Y` int(5) NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `event` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  `start_Time` int(10) NOT NULL DEFAULT '-1',
  `end_Time` int(10) NOT NULL DEFAULT '-1',
  `state` int(10) NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `house_portal` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `house_Id` int(10) unsigned NOT NULL,
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `landblock_instance` (
  `guid` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `landblock` int(5) GENERATED ALWAYS AS ((`obj_Cell_Id` >> 16)) VIRTUAL,
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  `is_Link_Child` bit(1) NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`guid`)
) ENGINE=InnoDB;
CREATE TABLE `landblock_instance_link` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `parent_GUID` int(10) unsigned NOT NULL,
  `child_GUID` int(10) unsigned NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `points_of_interest` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `name` text NOT NULL,
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `quest` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  `min_Delta` int(10) unsigned NOT NULL,
  `max_Solves` int(10) NOT NULL,
  `message` text,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe` (
  `id` int(10) unsigned NOT NULL,
  `unknown_1` int(10) unsigned NOT NULL,
  `skill` int(10) unsigned NOT NULL,
  `difficulty` int(10) unsigned NOT NULL,
  `salvage_Type` int(10) unsigned NOT NULL,
  `success_W_C_I_D` int(10) unsigned NOT NULL,
  `success_Amount` int(10) unsigned NOT NULL,
  `success_Message` text,
  `fail_W_C_I_D` int(10) unsigned NOT NULL,
  `fail_Amount` int(10) unsigned NOT NULL,
  `fail_Message` text,
  `success_Destroy_Source_Chance` double NOT NULL,
  `success_Destroy_Source_Amount` int(10) unsigned NOT NULL,
  `success_Destroy_Source_Message` text,
  `success_Destroy_Target_Chance` double NOT NULL,
  `success_Destroy_Target_Amount` int(10) unsigned NOT NULL,
  `success_Destroy_Target_Message` text,
  `fail_Destroy_Source_Chance` double NOT NULL,
  `fail_Destroy_Source_Amount` int(10) unsigned NOT NULL,
  `fail_Destroy_Source_Message` text,
  `fail_Destroy_Target_Chance` double NOT NULL,
  `fail_Destroy_Target_Amount` int(10) unsigned NOT NULL,
  `fail_Destroy_Target_Message` text,
  `data_Id` int(10) unsigned NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mod` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `executes_On_Success` bit(1) NOT NULL,
  `health` int(10) NOT NULL,
  `stamina` int(10) NOT NULL,
  `mana` int(10) NOT NULL,
  `unknown_7` bit(1) NOT NULL,
  `data_Id` int(10) NOT NULL,
  `unknown_9` int(10) NOT NULL,
  `instance_Id` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` double NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_mods_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Mod_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` text,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` double NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `recipe_requirements_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `recipe_Id` int(10) unsigned NOT NULL,
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` text,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `spell` (
  `id` int(10) unsigned NOT NULL,
  `name` text NOT NULL,
  `stat_Mod_Type` int(10) unsigned DEFAULT NULL,
  `stat_Mod_Key` int(10) unsigned DEFAULT NULL,
  `stat_Mod_Val` float DEFAULT NULL,
  `e_Type` int(10) unsigned DEFAULT NULL,
  `base_Intensity` int(10) DEFAULT NULL,
  `variance` int(10) DEFAULT NULL,
  `wcid` int(10) unsigned DEFAULT NULL,
  `num_Projectiles` int(10) DEFAULT NULL,
  `num_Projectiles_Variance` int(10) DEFAULT NULL,
  `spread_Angle` float DEFAULT NULL,
  `vertical_Angle` float DEFAULT NULL,
  `default_Launch_Angle` float DEFAULT NULL,
  `non_Tracking` bit(1) DEFAULT NULL,
  `create_Offset_Origin_X` float DEFAULT NULL,
  `create_Offset_Origin_Y` float DEFAULT NULL,
  `create_Offset_Origin_Z` float DEFAULT NULL,
  `padding_Origin_X` float DEFAULT NULL,
  `padding_Origin_Y` float DEFAULT NULL,
  `padding_Origin_Z` float DEFAULT NULL,
  `dims_Origin_X` float DEFAULT NULL,
  `dims_Origin_Y` float DEFAULT NULL,
  `dims_Origin_Z` float DEFAULT NULL,
  `peturbation_Origin_X` float DEFAULT NULL,
  `peturbation_Origin_Y` float DEFAULT NULL,
  `peturbation_Origin_Z` float DEFAULT NULL,
  `imbued_Effect` int(10) unsigned DEFAULT NULL,
  `slayer_Creature_Type` int(10) DEFAULT NULL,
  `slayer_Damage_Bonus` float DEFAULT NULL,
  `crit_Freq` double DEFAULT NULL,
  `crit_Multiplier` double DEFAULT NULL,
  `ignore_Magic_Resist` int(10) DEFAULT NULL,
  `elemental_Modifier` double DEFAULT NULL,
  `drain_Percentage` float DEFAULT NULL,
  `damage_Ratio` float DEFAULT NULL,
  `damage_Type` int(10) DEFAULT NULL,
  `boost` int(10) DEFAULT NULL,
  `boost_Variance` int(10) DEFAULT NULL,
  `source` int(10) DEFAULT NULL,
  `destination` int(10) DEFAULT NULL,
  `proportion` float DEFAULT NULL,
  `loss_Percent` float DEFAULT NULL,
  `source_Loss` int(10) DEFAULT NULL,
  `transfer_Cap` int(10) DEFAULT NULL,
  `max_Boost_Allowed` int(10) DEFAULT NULL,
  `transfer_Bitfield` int(10) unsigned DEFAULT NULL,
  `index` int(10) DEFAULT NULL,
  `link` int(10) DEFAULT NULL,
  `position_Obj_Cell_ID` int(10) unsigned DEFAULT NULL,
  `position_Origin_X` float DEFAULT NULL,
  `position_Origin_Y` float DEFAULT NULL,
  `position_Origin_Z` float DEFAULT NULL,
  `position_Angles_W` float DEFAULT NULL,
  `position_Angles_X` float DEFAULT NULL,
  `position_Angles_Y` float DEFAULT NULL,
  `position_Angles_Z` float DEFAULT NULL,
  `min_Power` int(10) DEFAULT NULL,
  `max_Power` int(10) DEFAULT NULL,
  `power_Variance` float DEFAULT NULL,
  `dispel_School` int(10) DEFAULT NULL,
  `align` int(10) DEFAULT NULL,
  `number` int(10) DEFAULT NULL,
  `number_Variance` float DEFAULT NULL,
  `dot_Duration` double DEFAULT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `treasure_death` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `treasure_Type` int(10) unsigned NOT NULL,
  `tier` int(10) NOT NULL,
  `loot_Quality_Mod` float NOT NULL,
  `unknown_Chances` int(10) NOT NULL,
  `item_Chance` int(10) NOT NULL,
  `item_Min_Amount` int(10) NOT NULL,
  `item_Max_Amount` int(10) NOT NULL,
  `item_Treasure_Type_Selection_Chances` int(10) NOT NULL,
  `magic_Item_Chance` int(10) NOT NULL,
  `magic_Item_Min_Amount` int(10) NOT NULL,
  `magic_Item_Max_Amount` int(10) NOT NULL,
  `magic_Item_Treasure_Type_Selection_Chances` int(10) NOT NULL,
  `mundane_Item_Chance` int(10) NOT NULL,
  `mundane_Item_Min_Amount` int(10) NOT NULL,
  `mundane_Item_Max_Amount` int(10) NOT NULL,
  `mundane_Item_Type_Selection_Chances` int(10) NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `treasure_gem_count` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `gem_Code` tinyint(3) unsigned NOT NULL,
  `tier` int(11) NOT NULL,
  `count` int(11) NOT NULL,
  `chance` float NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf16;
CREATE TABLE `treasure_material_base` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Code` int(10) unsigned NOT NULL,
  `tier` int(10) unsigned NOT NULL,
  `probability` float NOT NULL,
  `material_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `treasure_material_color` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Id` int(10) unsigned NOT NULL,
  `color_Code` int(10) unsigned NOT NULL,
  `palette_Template` int(10) unsigned NOT NULL,
  `probability` float NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `treasure_material_groups` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Group` int(10) unsigned NOT NULL,
  `tier` int(10) unsigned NOT NULL,
  `probability` float NOT NULL,
  `material_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `treasure_wielded` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `treasure_Type` int(10) unsigned NOT NULL,
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `palette_Id` int(10) unsigned NOT NULL,
  `unknown_1` int(10) unsigned NOT NULL,
  `shade` float NOT NULL COMMENT 'Shade of Object\'s Palette',
  `stack_Size` int(10) NOT NULL DEFAULT '1',
  `stack_Size_Variance` float NOT NULL,
  `probability` float NOT NULL,
  `unknown_3` int(10) unsigned NOT NULL,
  `unknown_4` int(10) unsigned NOT NULL,
  `unknown_5` int(10) unsigned NOT NULL,
  `set_Start` bit(1) NOT NULL,
  `has_Sub_Set` bit(1) NOT NULL,
  `continues_Previous_Set` bit(1) NOT NULL,
  `unknown_9` int(10) unsigned NOT NULL,
  `unknown_10` int(10) unsigned NOT NULL,
  `unknown_11` int(10) unsigned NOT NULL,
  `unknown_12` int(10) unsigned NOT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `version` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `base_Version` varchar(45) DEFAULT NULL,
  `patch_Version` varchar(45) DEFAULT NULL,
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
";

const SCHEMA_B: &str = r"CREATE TABLE `weenie` (
  `class_Id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `class_Name` varchar(100) NOT NULL,
  `type` int(5) NOT NULL DEFAULT '0',
  `last_Modified` datetime NOT NULL,
  PRIMARY KEY (`class_Id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_anim_part` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `index` tinyint(3) unsigned NOT NULL,
  `animation_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_attribute` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `init_Level` int(10) unsigned NOT NULL,
  `level_From_C_P` int(10) unsigned NOT NULL,
  `c_P_Spent` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_attribute_2nd` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `init_Level` int(10) unsigned NOT NULL,
  `level_From_C_P` int(10) unsigned NOT NULL,
  `c_P_Spent` int(10) unsigned NOT NULL,
  `current_Level` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_body_part` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `key` smallint(5) unsigned NOT NULL,
  `d_Type` int(10) NOT NULL,
  `d_Val` int(10) NOT NULL,
  `d_Var` float NOT NULL,
  `base_Armor` int(10) NOT NULL,
  `armor_Vs_Slash` int(10) NOT NULL,
  `armor_Vs_Pierce` int(10) NOT NULL,
  `armor_Vs_Bludgeon` int(10) NOT NULL,
  `armor_Vs_Cold` int(10) NOT NULL,
  `armor_Vs_Fire` int(10) NOT NULL,
  `armor_Vs_Acid` int(10) NOT NULL,
  `armor_Vs_Electric` int(10) NOT NULL,
  `armor_Vs_Nether` int(10) NOT NULL,
  `b_h` int(10) NOT NULL,
  `h_l_f` float NOT NULL,
  `m_l_f` float NOT NULL,
  `l_l_f` float NOT NULL,
  `h_r_f` float NOT NULL,
  `m_r_f` float NOT NULL,
  `l_r_f` float NOT NULL,
  `h_l_b` float NOT NULL,
  `m_l_b` float NOT NULL,
  `l_l_b` float NOT NULL,
  `h_r_b` float NOT NULL,
  `m_r_b` float NOT NULL,
  `l_r_b` float NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `max_Num_Pages` int(10) NOT NULL DEFAULT '1',
  `max_Num_Chars_Per_Page` int(10) NOT NULL DEFAULT '1000',
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_book_page_data` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `page_Id` int(10) unsigned NOT NULL,
  `author_Id` int(10) unsigned NOT NULL,
  `author_Name` varchar(255) NOT NULL DEFAULT '',
  `author_Account` varchar(255) NOT NULL DEFAULT 'prewritten',
  `ignore_Author` bit(1) NOT NULL,
  `page_Text` text NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` bit(1) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_create_list` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `destination_Type` tinyint(5) NOT NULL,
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `stack_Size` int(10) NOT NULL DEFAULT '1',
  `palette` tinyint(5) NOT NULL,
  `shade` float NOT NULL,
  `try_To_Bond` bit(1) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_emote` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `category` int(10) unsigned NOT NULL,
  `probability` float NOT NULL DEFAULT '1',
  `weenie_Class_Id` int(10) unsigned DEFAULT NULL,
  `style` int(10) unsigned DEFAULT NULL,
  `substyle` int(10) unsigned DEFAULT NULL,
  `quest` text,
  `vendor_Type` int(10) DEFAULT NULL,
  `min_Health` float DEFAULT NULL,
  `max_Health` float DEFAULT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_emote_action` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `emote_Id` int(10) unsigned NOT NULL,
  `order` int(10) unsigned NOT NULL,
  `type` int(10) unsigned NOT NULL,
  `delay` float NOT NULL DEFAULT '1',
  `extent` float NOT NULL DEFAULT '1',
  `motion` int(10) unsigned DEFAULT NULL,
  `message` text,
  `test_String` text,
  `min` int(10) DEFAULT NULL,
  `max` int(10) DEFAULT NULL,
  `min_64` bigint(10) DEFAULT NULL,
  `max_64` bigint(10) DEFAULT NULL,
  `min_Dbl` double DEFAULT NULL,
  `max_Dbl` double DEFAULT NULL,
  `stat` int(10) DEFAULT NULL,
  `display` bit(1) DEFAULT NULL,
  `amount` int(10) DEFAULT NULL,
  `amount_64` bigint(10) DEFAULT NULL,
  `hero_X_P_64` bigint(10) DEFAULT NULL,
  `percent` double DEFAULT NULL,
  `spell_Id` int(10) DEFAULT NULL,
  `wealth_Rating` int(10) DEFAULT NULL,
  `treasure_Class` int(10) DEFAULT NULL,
  `treasure_Type` int(10) DEFAULT NULL,
  `p_Script` int(10) DEFAULT NULL,
  `sound` int(10) DEFAULT NULL,
  `destination_Type` tinyint(5) DEFAULT NULL,
  `weenie_Class_Id` int(10) unsigned DEFAULT NULL,
  `stack_Size` int(10) DEFAULT NULL,
  `palette` int(10) DEFAULT NULL,
  `shade` float DEFAULT NULL,
  `try_To_Bond` bit(1) DEFAULT NULL,
  `obj_Cell_Id` int(10) unsigned DEFAULT NULL,
  `origin_X` float DEFAULT NULL,
  `origin_Y` float DEFAULT NULL,
  `origin_Z` float DEFAULT NULL,
  `angles_W` float DEFAULT NULL,
  `angles_X` float DEFAULT NULL,
  `angles_Y` float DEFAULT NULL,
  `angles_Z` float DEFAULT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_event_filter` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `event` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` double NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_generator` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `probability` float NOT NULL DEFAULT '1',
  `weenie_Class_Id` int(10) unsigned NOT NULL,
  `delay` float DEFAULT NULL,
  `init_Create` int(10) NOT NULL DEFAULT '1',
  `max_Create` int(10) NOT NULL DEFAULT '1',
  `when_Create` int(10) unsigned NOT NULL DEFAULT '2',
  `where_Create` int(10) unsigned NOT NULL DEFAULT '4',
  `stack_Size` int(10) DEFAULT NULL,
  `palette_Id` int(10) unsigned DEFAULT NULL,
  `shade` float DEFAULT NULL,
  `obj_Cell_Id` int(10) unsigned DEFAULT NULL,
  `origin_X` float DEFAULT NULL,
  `origin_Y` float DEFAULT NULL,
  `origin_Z` float DEFAULT NULL,
  `angles_W` float DEFAULT NULL,
  `angles_X` float DEFAULT NULL,
  `angles_Y` float DEFAULT NULL,
  `angles_Z` float DEFAULT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` int(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_int64` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` bigint(10) NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_palette` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `sub_Palette_Id` int(10) unsigned NOT NULL,
  `offset` smallint(5) unsigned NOT NULL,
  `length` smallint(5) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_position` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `position_Type` smallint(5) unsigned NOT NULL,
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_skill` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `level_From_P_P` smallint(5) unsigned NOT NULL,
  `s_a_c` int(10) unsigned NOT NULL,
  `p_p` int(10) unsigned NOT NULL,
  `init_Level` int(10) unsigned NOT NULL,
  `resistance_At_Last_Check` int(10) unsigned NOT NULL,
  `last_Used_Time` double NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_spell_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `spell` int(10) NOT NULL,
  `probability` float NOT NULL DEFAULT '2',
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` text NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
CREATE TABLE `weenie_properties_texture_map` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `index` tinyint(3) unsigned NOT NULL,
  `old_Id` int(10) unsigned NOT NULL,
  `new_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
";

/// Rows. `{T}`/`{F}` are `bit(1)` true/false. Weenies: 100 a creature, 200 a book, 300 a scroll
/// (DID Spell 1234), 400 a slum lord, 500 a generic item carrying creature and book rows.
fn rows() -> String {
    let d = "'2021-11-01 00:00:00'";
    let mut s = String::new();
    fn push(s: &mut String, table: &str, rows: &[String]) {
        s.push_str(&format!(
            "INSERT INTO `{table}` VALUES {};\n",
            rows.join(",")
        ));
    }
    let r = |x: &str| x.replace("{D}", d);

    push(
        &mut s,
        "weenie",
        &[
            r("(100,'drudgetest',10,{D})"),
            r("(200,'booktest',8,{D})"),
            r("(300,'scrolltest',34,{D})"),
        ],
    );
    // A second INSERT for the same table: row numbering and appending continue.
    push(
        &mut s,
        "weenie",
        &[
            r("(400,'slumlord_Cottage_test',55,{D})"),
            r("(500,'itemtest',1,{D})"),
        ],
    );

    push(
        &mut s,
        "weenie_properties_int",
        &[
            "(1,100,25,5)".into(),
            "(2,100,1,16)".into(),
            "(3,500,25,-7)".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_int64",
        &["(1,100,1,-9000000000)".into()],
    );
    push(
        &mut s,
        "weenie_properties_float",
        &["(1,100,39,1.5)".into(), "(2,100,1,0.1)".into()],
    );
    push(
        &mut s,
        "weenie_properties_bool",
        &["(1,100,1,{T})".into(), "(2,100,2,{F})".into()],
    );
    push(
        &mut s,
        "weenie_properties_d_i_d",
        &[
            "(1,100,1,33554433)".into(),
            "(2,300,28,1234)".into(),
            "(3,300,1,33554434)".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_i_i_d",
        &["(1,100,2,2147483649)".into()],
    );
    push(
        &mut s,
        "weenie_properties_string",
        &[
            // Every mysqldump escape: \' \" \\ \n \r \t \Z \0, and a parenthesis pair inside a string.
            "(1,100,1,'Drudge \\'Tester\\' \\\"(x),(y)\\\"')".into(),
            "(2,100,16,'line1\\nline2\\r\\t\\\\ \\Z\\0end')".into(),
            "(3,300,1,'Scroll of Test')".into(),
            "(4,400,1,'Cottage')".into(),
            "(5,500,1,'Thing')".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_position",
        &["(1,100,1,2847146009,84,7.1,94.005,0.996,0,0,-0.08)".into()],
    );
    push(
        &mut s,
        "weenie_properties_attribute",
        &[
            "(1,100,2,20,0,0)".into(),
            "(2,100,1,10,0,0)".into(),
            "(3,500,1,99,0,0)".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_attribute_2nd",
        &["(1,100,1,15,0,0,15)".into()],
    );
    push(&mut s, "weenie_properties_body_part", &[
        "(1,100,0,4,2,0.75,5,6,7,8,9,10,11,12,13,1,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1,1.1,1.2)".into(),
    ]);
    push(
        &mut s,
        "weenie_properties_skill",
        &[
            "(1,100,6,0,2,0,10,0,0)".into(),
            "(2,500,6,0,2,0,10,0,0)".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_create_list",
        &[
            "(1,100,2,273,25,0,0,{F})".into(),
            "(2,100,8,628,1,-3,0.5,{T})".into(),
        ],
    );
    push(
        &mut s,
        "weenie_properties_emote",
        &[
            "(1,100,1,1,NULL,NULL,NULL,'quest\\'s',NULL,NULL,NULL)".into(),
            "(2,100,13,0.5,300,2147483709,1090519043,NULL,-2,0.25,0.75)".into(),
        ],
    );
    // Stored with the higher `order` first: ConvertToEntityWeenie sorts by `order`.
    push(&mut s, "weenie_properties_emote_action", &[
        "(1,1,1,10,0,1,NULL,'second',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL)".into(),
        "(2,1,0,1,0.5,2,318767235,'first','test',1,2,-5000000000,5000000000,0.5,1.5,3,{T},4,6000000000,7000000000,0.25,1234,5,6,7,-1,-2,-3,300,9,10,0.5,{F},2847146009,1,2,3,1,0,0,0)".into(),
    ]);
    push(
        &mut s,
        "weenie_properties_event_filter",
        &["(1,100,94)".into(), "(2,100,414)".into()],
    );
    push(
        &mut s,
        "weenie_properties_generator",
        &[
            "(1,100,0.5,300,NULL,1,2,2,4,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL)"
                .into(),
            "(2,100,1,200,120,1,1,1,2,5,3,0.25,2847146009,1,2,3,1,0,0,0)".into(),
        ],
    );
    // Out of spell order in id order; GetWeenie orders the spell book by id.
    push(
        &mut s,
        "weenie_properties_spell_book",
        &["(1,100,2000,2.5)".into(), "(2,100,1000,2.25)".into()],
    );
    push(
        &mut s,
        "weenie_properties_anim_part",
        &["(1,100,3,16777217)".into()],
    );
    push(
        &mut s,
        "weenie_properties_palette",
        &["(1,100,67108865,0,24)".into()],
    );
    // Stored with the higher part index first: GetWeenie returns them in index order.
    push(
        &mut s,
        "weenie_properties_texture_map",
        &[
            "(1,100,9,83886081,83886082)".into(),
            "(2,100,1,83886083,83886084)".into(),
        ],
    );
    // A book row on a non-book weenie (500): GetWeenie drops it, GetAllWeenies keeps it.
    push(
        &mut s,
        "weenie_properties_book",
        &["(1,200,2,1000)".into(), "(2,500,1,1)".into()],
    );
    // Pages stored in descending page order: the converter sorts them by page id.
    push(
        &mut s,
        "weenie_properties_book_page_data",
        &[
            "(1,200,1,4294967295,'Author','prewritten',{F},'page two')".into(),
            "(2,200,0,0,'','prewritten',{T},'page one')".into(),
        ],
    );

    // Landblock instances: an explicit column list (the generated `landblock` column is left
    // out), deliberately not in the table's column order. Guid order differs from landblock order.
    s.push_str(&format!(
        "INSERT INTO `landblock_instance` (`weenie_Class_Id`, `guid`, `obj_Cell_Id`, `origin_X`, `origin_Y`,          `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES          (400,2056994817,2847146008,73.2987,3.15926,0.005,0.174196,0,0,0.984711,{F},{d}),         (11730,2056994818,2847146008,1,2,3,1,0,0,0,{T},{d}),         (278,2056994819,2847146009,1,2,3,1,0,0,0,{F},{d}),         (100,2056994820,2847146009,1,2,3,1,0,0,0,{F},{d}),         (100,2147418113,131073,1,2,3,1,0,0,0,{F},{d}),         (100,1879052288,66247,1,2,3,1,0,0,0,{F},{d});
"
    ));
    push(
        &mut s,
        "landblock_instance_link",
        &[r("(1,2056994817,2056994818,{D})")],
    );

    push(
        &mut s,
        "encounter",
        &[
            r("(1,5,100,3,1,{D})"),
            r("(2,5,200,1,2,{D})"),
            r("(3,6,100,0,0,{D})"),
        ],
    );
    push(
        &mut s,
        "cook_book",
        &[
            r("(10,1,100,200,{D})"),
            r("(5,2,100,200,{D})"),
            r("(11,2,300,400,{D})"),
        ],
    );
    push(
        &mut s,
        "recipe",
        &[
            r("(1,0,0,0,0,300,1,'yes',0,0,NULL,0.5,1,NULL,1,1,'gone',0,0,NULL,0,0,NULL,0,{D})"),
            r("(2,0,35,100,0,0,0,NULL,0,0,'no',0,0,NULL,0,0,NULL,1,1,NULL,1,1,NULL,7,{D})"),
        ],
    );
    push(&mut s, "recipe_mod", &["(1,2,{T},1,2,3,{F},4,5,6)".into()]);
    push(&mut s, "recipe_mods_bool", &["(1,1,0,1,{T},2,3)".into()]);
    push(
        &mut s,
        "recipe_mods_d_i_d",
        &["(1,1,1,2,4294967295,3,4)".into()],
    );
    push(&mut s, "recipe_mods_float", &["(1,1,2,3,0.125,4,5)".into()]);
    push(&mut s, "recipe_mods_i_i_d", &["(1,1,3,4,7,5,6)".into()]);
    push(&mut s, "recipe_mods_int", &["(1,1,-1,5,-8,6,7)".into()]);
    push(&mut s, "recipe_mods_string", &["(1,1,5,6,NULL,7,8)".into()]);
    push(
        &mut s,
        "recipe_requirements_bool",
        &["(1,2,0,1,{F},2,'must be false')".into()],
    );
    push(
        &mut s,
        "recipe_requirements_d_i_d",
        &["(1,2,0,1,9,2,NULL)".into()],
    );
    push(
        &mut s,
        "recipe_requirements_float",
        &["(1,2,0,1,2.5,2,NULL)".into()],
    );
    push(
        &mut s,
        "recipe_requirements_i_i_d",
        &["(1,2,0,1,8,2,NULL)".into()],
    );
    push(
        &mut s,
        "recipe_requirements_int",
        &["(1,2,0,1,-4,2,'too low')".into()],
    );
    push(
        &mut s,
        "recipe_requirements_string",
        &["(1,2,0,1,'abc',2,NULL)".into()],
    );

    push(
        &mut s,
        "event",
        &[
            r("(1,'EventOne',-1,-1,1,{D})"),
            r("(2,'Event Two',100,200,2,{D})"),
        ],
    );
    push(
        &mut s,
        "house_portal",
        &[
            r("(1,77,2847146009,1,2,3,1,0,0,0,{D})"),
            r("(2,77,2847146008,4,5,6,1,0,0,0,{D})"),
            // Just below the midpoint of two floats: direct decimal-to-float rounds down, MySQL's
            // decimal-to-double-to-float lands on the midpoint and rounds to even (up).
            r("(3,78,66247,1.0000001788139343261718749,8,9,1,0,0,0,{D})"),
        ],
    );
    push(&mut s, "points_of_interest", &[r("(1,'Holtburg',300,{D})")]);
    push(
        &mut s,
        "quest",
        &[
            r("(1,'TestQuest',3600,-1,NULL,{D})"),
            r("(2,'OtherQuest',0,5,'done',{D})"),
        ],
    );
    push(&mut s, "spell", &[
        r("(1234,'Test Spell',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,{D})"),
        r("(1235,'Full Spell',1,2,3.5,4,5,6,7,8,9,10.5,11.5,12.5,{T},1,2,3,4,5,6,7,8,9,10,11,12,13,14,15.5,16.5,17.5,18,19.5,20.5,21.5,22,23,24,25,26,27.5,28.5,29,30,31,32,33,34,2847146009,1,2,3,1,0,0,0,35,36,37.5,38,39,40,41.5,42.5,{D})"),
    ]);
    push(
        &mut s,
        "treasure_death",
        &[
            r("(1,5,1,0.25,0,100,1,2,8,50,1,1,9,50,1,2,10,{D})"),
            r("(2,6,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,{D})"),
        ],
    );
    push(&mut s, "treasure_gem_count", &["(1,2,1,3,0.5)".into()]);
    // Probabilities 1 and 1 normalize to 0.5 each; the zero row is dropped by `Probability > 0`.
    push(
        &mut s,
        "treasure_material_base",
        &[
            "(1,1,1,1,11)".into(),
            "(2,1,1,1,12)".into(),
            "(3,1,1,0,13)".into(),
            "(4,1,2,1,14)".into(),
        ],
    );
    // 0.01 + 0.04 + 0.02 sums differently in float (0.069999993) and in double (0.07).
    push(
        &mut s,
        "treasure_material_color",
        &[
            "(1,11,3,100,0.2)".into(),
            "(2,11,3,101,0.2)".into(),
            "(3,12,4,102,0.01)".into(),
            "(4,12,4,103,0.04)".into(),
            "(5,12,4,104,0.02)".into(),
        ],
    );
    push(
        &mut s,
        "treasure_material_groups",
        &["(1,40,1,0.5,11)".into(), "(2,40,1,0.5,12)".into()],
    );
    push(
        &mut s,
        "treasure_wielded",
        &[
            r("(1,9,300,0,0,0,1,0,0.5,0,0,0,{T},{F},{F},0,0,0,0,{D})"),
            r("(2,9,400,0,0,0.5,2,0.25,0.5,0,0,0,{F},{T},{T},0,0,0,0,{D})"),
            r("(3,10,500,0,0,0,1,0,1,0,0,0,{F},{F},{F},0,0,0,0,{D})"),
        ],
    );
    push(&mut s, "version", &[r("(1,'v0.8.8','v0.9.294',{D})")]);
    s
}

/// The fixture imported once per test process, and the database over it.
pub fn imported() -> &'static (Vec<u8>, Imported) {
    static CELL: OnceLock<(Vec<u8>, Imported)> = OnceLock::new();
    CELL.get_or_init(|| import(dump().as_bytes()).expect("fixture imports"))
}

/// A fresh database (fresh caches) over the fixture's pack.
#[must_use]
pub fn db() -> PackContent {
    PackContent::new(Pack::from_bytes(imported().0.clone()).expect("fixture pack opens"))
}
