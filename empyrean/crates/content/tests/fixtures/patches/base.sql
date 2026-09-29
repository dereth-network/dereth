-- The base for serv-content's patch tests, in mysqldump's format.
-- Schema: the 54 CREATE TABLE statements of ACE-World-Database-v0.9.294.sql (ACE-World-16PY-Patches,
-- AGPL-3.0), verbatim, extracted by unit 8b.1 (O7). Rows: hand-written, a few per table the tests touch.
/*!40101 SET NAMES utf8 */;
USE `ace_world`;
CREATE TABLE `cook_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this cook book instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `source_W_C_I_D` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of the source object for this recipe',
  `target_W_C_I_D` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of the target object for this recipe',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `recipe_source_target_uidx` (`recipe_Id`,`source_W_C_I_D`,`target_W_C_I_D`),
  KEY `source_idx` (`source_W_C_I_D`),
  KEY `target_idx` (`target_W_C_I_D`),
  CONSTRAINT `cookbook_recipe` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=55930 DEFAULT CHARSET=utf8 COMMENT='Cook Book for Recipes';
CREATE TABLE `encounter` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Encounter',
  `landblock` int(5) NOT NULL COMMENT 'Landblock for this Encounter',
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of generator/object to spawn for Encounter',
  `cell_X` int(5) NOT NULL COMMENT 'CellX position of this Encounter',
  `cell_Y` int(5) NOT NULL COMMENT 'CellY position of this Encounter',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `landblock_cellx_celly_uidx` (`landblock`,`cell_X`,`cell_Y`),
  KEY `landblock_idx` (`landblock`)
) ENGINE=InnoDB AUTO_INCREMENT=167042 DEFAULT CHARSET=utf8 COMMENT='Encounters';
CREATE TABLE `event` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Event',
  `name` varchar(255) NOT NULL COMMENT 'Unique Event of Quest',
  `start_Time` int(10) NOT NULL DEFAULT '-1' COMMENT 'Unixtime of Event Start',
  `end_Time` int(10) NOT NULL DEFAULT '-1' COMMENT 'Unixtime of Event End',
  `state` int(10) NOT NULL COMMENT 'State of Event (GameEventState)',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `name_UNIQUE` (`name`)
) ENGINE=InnoDB AUTO_INCREMENT=721 DEFAULT CHARSET=utf8 COMMENT='Events';
CREATE TABLE `house_portal` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this House Portal',
  `house_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of House',
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `house_Id_UNIQUE` (`house_Id`,`obj_Cell_Id`)
) ENGINE=InnoDB AUTO_INCREMENT=862 DEFAULT CHARSET=utf8 COMMENT='House Portal Destinations';
CREATE TABLE `landblock_instance` (
  `guid` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Instance',
  `landblock` int(5) GENERATED ALWAYS AS ((`obj_Cell_Id` >> 16)) VIRTUAL,
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of object to spawn',
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  `is_Link_Child` bit(1) NOT NULL COMMENT 'Is this a child link for any other instances?',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`guid`),
  KEY `instance_landblock_idx` (`landblock`)
) ENGINE=InnoDB AUTO_INCREMENT=2142085204 DEFAULT CHARSET=utf8 COMMENT='Weenie Instances for each Landblock';
CREATE TABLE `landblock_instance_link` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Instance Link',
  `parent_GUID` int(10) unsigned NOT NULL COMMENT 'GUID of parent instance',
  `child_GUID` int(10) unsigned NOT NULL COMMENT 'GUID of child instance',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `parent_child_uidx` (`parent_GUID`,`child_GUID`),
  KEY `child_idx` (`child_GUID`),
  CONSTRAINT `instance_link` FOREIGN KEY (`parent_GUID`) REFERENCES `landblock_instance` (`guid`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=311119 DEFAULT CHARSET=utf8 COMMENT='Weenie Instance Links';
CREATE TABLE `points_of_interest` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this POI',
  `name` text NOT NULL COMMENT 'Name for POI',
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of portal weenie to reference for destination of POI',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `name_UNIQUE` (`name`(100))
) ENGINE=InnoDB AUTO_INCREMENT=106 DEFAULT CHARSET=utf8 COMMENT='Points of Interest for @telepoi command';
CREATE TABLE `quest` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Quest',
  `name` varchar(255) NOT NULL COMMENT 'Unique Name of Quest',
  `min_Delta` int(10) unsigned NOT NULL COMMENT 'Minimum time between Quest completions',
  `max_Solves` int(10) NOT NULL COMMENT 'Maximum number of times Quest can be completed',
  `message` text COMMENT 'Quest solved text - unused?',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `name_UNIQUE` (`name`)
) ENGINE=InnoDB AUTO_INCREMENT=5301 DEFAULT CHARSET=utf8 COMMENT='Quests';
CREATE TABLE `recipe` (
  `id` int(10) unsigned NOT NULL COMMENT 'Unique Id of this Recipe',
  `unknown_1` int(10) unsigned NOT NULL,
  `skill` int(10) unsigned NOT NULL,
  `difficulty` int(10) unsigned NOT NULL,
  `salvage_Type` int(10) unsigned NOT NULL,
  `success_W_C_I_D` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of object to create upon successful application of this recipe',
  `success_Amount` int(10) unsigned NOT NULL COMMENT 'Amount of objects to create upon successful application of this recipe',
  `success_Message` text,
  `fail_W_C_I_D` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of object to create upon failing application of this recipe',
  `fail_Amount` int(10) unsigned NOT NULL COMMENT 'Amount of objects to create upon failing application of this recipe',
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
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 COMMENT='Recipes';
CREATE TABLE `recipe_mod` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `executes_On_Success` bit(1) NOT NULL,
  `health` int(10) NOT NULL,
  `stamina` int(10) NOT NULL,
  `mana` int(10) NOT NULL,
  `unknown_7` bit(1) NOT NULL,
  `data_Id` int(10) NOT NULL,
  `unknown_9` int(10) NOT NULL,
  `instance_Id` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_Mod` (`recipe_Id`),
  CONSTRAINT `recipeId_Mod` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=1522 DEFAULT CHARSET=utf8 COMMENT='Recipe Mods';
CREATE TABLE `recipe_mods_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_bool` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_bool` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=15 DEFAULT CHARSET=utf8 COMMENT='Recipe Bool Mods';
CREATE TABLE `recipe_mods_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_did` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_did` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=1004 DEFAULT CHARSET=utf8 COMMENT='Recipe DID Mods';
CREATE TABLE `recipe_mods_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` double NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_float` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_float` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=502 DEFAULT CHARSET=utf8 COMMENT='Recipe Float Mods';
CREATE TABLE `recipe_mods_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_iid` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_iid` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=488 DEFAULT CHARSET=utf8 COMMENT='Recipe IID Mods';
CREATE TABLE `recipe_mods_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) NOT NULL,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_int` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_int` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=528 DEFAULT CHARSET=utf8 COMMENT='Recipe Int Mods';
CREATE TABLE `recipe_mods_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` text,
  `enum` int(10) NOT NULL,
  `source` int(10) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_string` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_string` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=893 DEFAULT CHARSET=utf8 COMMENT='Recipe String Mods';
CREATE TABLE `recipe_requirements_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_bool` (`recipe_Id`),
  CONSTRAINT `recipeId_req_bool` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=7 DEFAULT CHARSET=utf8 COMMENT='Recipe Bool Requirments';
CREATE TABLE `recipe_requirements_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_did` (`recipe_Id`),
  CONSTRAINT `recipeId_req_did` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=5 DEFAULT CHARSET=utf8 COMMENT='Recipe DID Requirments';
CREATE TABLE `recipe_requirements_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` double NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_float` (`recipe_Id`),
  CONSTRAINT `recipeId_req_float` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=3 DEFAULT CHARSET=utf8 COMMENT='Recipe Float Requirments';
CREATE TABLE `recipe_requirements_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) unsigned NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_iid` (`recipe_Id`),
  CONSTRAINT `recipeId_req_iid` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=4 DEFAULT CHARSET=utf8 COMMENT='Recipe IID Requirments';
CREATE TABLE `recipe_requirements_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` int(10) NOT NULL,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_int` (`recipe_Id`),
  CONSTRAINT `recipeId_req_int` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=959 DEFAULT CHARSET=utf8 COMMENT='Recipe Int Requirments';
CREATE TABLE `recipe_requirements_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int(10) unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint(5) NOT NULL,
  `stat` int(10) NOT NULL,
  `value` text,
  `enum` int(10) NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_string` (`recipe_Id`),
  CONSTRAINT `recipeId_req_string` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=8 DEFAULT CHARSET=utf8 COMMENT='Recipe String Requirments';
CREATE TABLE `spell` (
  `id` int(10) unsigned NOT NULL COMMENT 'Unique Id of this Spell',
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
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8 COMMENT='Spell Table Extended Data';
CREATE TABLE `treasure_death` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Treasure',
  `treasure_Type` int(10) unsigned NOT NULL COMMENT 'Type of Treasure for this instance',
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
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `treasureType_idx` (`treasure_Type`)
) ENGINE=InnoDB AUTO_INCREMENT=279 DEFAULT CHARSET=utf8 COMMENT='Death Treasure';
CREATE TABLE `treasure_gem_count` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `gem_Code` tinyint(3) unsigned NOT NULL,
  `tier` int(11) NOT NULL,
  `count` int(11) NOT NULL,
  `chance` float NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=487 DEFAULT CHARSET=utf16;
CREATE TABLE `treasure_material_base` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Code` int(10) unsigned NOT NULL COMMENT 'Derived from PropertyInt.TsysMutationData',
  `tier` int(10) unsigned NOT NULL COMMENT 'Loot Tier',
  `probability` float NOT NULL,
  `material_Id` int(10) unsigned NOT NULL COMMENT 'MaterialType',
  PRIMARY KEY (`id`),
  KEY `tier` (`tier`)
) ENGINE=InnoDB AUTO_INCREMENT=769 DEFAULT CHARSET=utf8;
CREATE TABLE `treasure_material_color` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Id` int(10) unsigned NOT NULL,
  `color_Code` int(10) unsigned NOT NULL,
  `palette_Template` int(10) unsigned NOT NULL,
  `probability` float NOT NULL,
  PRIMARY KEY (`id`),
  KEY `material_Id` (`material_Id`),
  KEY `tsys_Mutation_Color` (`color_Code`)
) ENGINE=InnoDB AUTO_INCREMENT=2188 DEFAULT CHARSET=utf8;
CREATE TABLE `treasure_material_groups` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `material_Group` int(10) unsigned NOT NULL COMMENT 'MaterialType Group',
  `tier` int(10) unsigned NOT NULL COMMENT 'Loot Tier',
  `probability` float NOT NULL,
  `material_Id` int(10) unsigned NOT NULL COMMENT 'MaterialType',
  PRIMARY KEY (`id`),
  KEY `tier` (`tier`)
) ENGINE=InnoDB AUTO_INCREMENT=433 DEFAULT CHARSET=utf8;
CREATE TABLE `treasure_wielded` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Treasure',
  `treasure_Type` int(10) unsigned NOT NULL COMMENT 'Type of Treasure for this instance',
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of Treasure to Generate',
  `palette_Id` int(10) unsigned NOT NULL COMMENT 'Palette Color of Object Generated',
  `unknown_1` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `shade` float NOT NULL COMMENT 'Shade of Object generated''s Palette',
  `stack_Size` int(10) NOT NULL DEFAULT '1' COMMENT 'Stack Size of object to create (-1 = infinite)',
  `stack_Size_Variance` float NOT NULL,
  `probability` float NOT NULL,
  `unknown_3` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_4` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_5` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `set_Start` bit(1) NOT NULL,
  `has_Sub_Set` bit(1) NOT NULL,
  `continues_Previous_Set` bit(1) NOT NULL,
  `unknown_9` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_10` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_11` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_12` int(10) unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `treasureType_idx` (`treasure_Type`)
) ENGINE=InnoDB AUTO_INCREMENT=6001 DEFAULT CHARSET=utf8 COMMENT='Wielded Treasure';
CREATE TABLE `version` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `base_Version` varchar(45) DEFAULT NULL,
  `patch_Version` varchar(45) DEFAULT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=2 DEFAULT CHARSET=utf8 COMMENT='Version Information';
CREATE TABLE `weenie` (
  `class_Id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Weenie Class Id (wcid) / (WCID) / (weenieClassId)',
  `class_Name` varchar(100) NOT NULL COMMENT 'Weenie Class Name (W_????_CLASS)',
  `type` int(5) NOT NULL DEFAULT '0' COMMENT 'WeenieType',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`class_Id`),
  UNIQUE KEY `className_UNIQUE` (`class_Name`)
) ENGINE=InnoDB AUTO_INCREMENT=90274 DEFAULT CHARSET=utf8 COMMENT='Weenies';
CREATE TABLE `weenie_properties_anim_part` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `index` tinyint(3) unsigned NOT NULL,
  `animation_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_index_uidx` (`object_Id`,`index`),
  CONSTRAINT `wcid_animpart` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=157 DEFAULT CHARSET=utf8 COMMENT='Animation Part Changes (from PCAPs) of Weenies';
CREATE TABLE `weenie_properties_attribute` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyAttribute.????)',
  `init_Level` int(10) unsigned NOT NULL COMMENT 'innate points',
  `level_From_C_P` int(10) unsigned NOT NULL COMMENT 'points raised',
  `c_P_Spent` int(10) unsigned NOT NULL COMMENT 'XP spent on this attribute',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_attribute_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_attribute` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=66157 DEFAULT CHARSET=utf8 COMMENT='Attribute Properties of Weenies';
CREATE TABLE `weenie_properties_attribute_2nd` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyAttribute2nd.????)',
  `init_Level` int(10) unsigned NOT NULL COMMENT 'innate points',
  `level_From_C_P` int(10) unsigned NOT NULL COMMENT 'points raised',
  `c_P_Spent` int(10) unsigned NOT NULL COMMENT 'XP spent on this attribute',
  `current_Level` int(10) unsigned NOT NULL COMMENT 'current value of the vital',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_attribute2nd_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_attribute2nd` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=33106 DEFAULT CHARSET=utf8 COMMENT='Attribute2nd (Vital) Properties of Weenies';
CREATE TABLE `weenie_properties_body_part` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `key` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertySkill.????)',
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
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_bodypart_type_uidx` (`object_Id`,`key`),
  CONSTRAINT `wcid_bodypart` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=83262 DEFAULT CHARSET=utf8 COMMENT='Body Part Properties of Weenies';
CREATE TABLE `weenie_properties_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `max_Num_Pages` int(10) NOT NULL DEFAULT '1' COMMENT 'Maximum number of pages per book',
  `max_Num_Chars_Per_Page` int(10) NOT NULL DEFAULT '1000' COMMENT 'Maximum number of characters per page',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_bookdata_uidx` (`object_Id`),
  CONSTRAINT `wcid_bookdata` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=1549 DEFAULT CHARSET=utf8 COMMENT='Book Properties of Weenies';
CREATE TABLE `weenie_properties_book_page_data` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the Book object this page belongs to',
  `page_Id` int(10) unsigned NOT NULL COMMENT 'Id of the page number for this page',
  `author_Id` int(10) unsigned NOT NULL COMMENT 'Id of the Author of this page',
  `author_Name` varchar(255) NOT NULL DEFAULT '' COMMENT 'Character Name of the Author of this page',
  `author_Account` varchar(255) NOT NULL DEFAULT 'prewritten' COMMENT 'Account Name of the Author of this page',
  `ignore_Author` bit(1) NOT NULL COMMENT 'if this is true, any character in the world can change the page',
  `page_Text` text NOT NULL COMMENT 'Text of the Page',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_pageid_uidx` (`object_Id`,`page_Id`),
  CONSTRAINT `wcid_pagedata` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=5039 DEFAULT CHARSET=utf8 COMMENT='Page Properties of Weenies';
CREATE TABLE `weenie_properties_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyBool.????)',
  `value` bit(1) NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_bool_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_bool` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=202924 DEFAULT CHARSET=utf8 COMMENT='Bool Properties of Weenies';
CREATE TABLE `weenie_properties_create_list` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `destination_Type` tinyint(5) NOT NULL COMMENT 'Type of Destination the value applies to (DestinationType.????)',
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of object to Create',
  `stack_Size` int(10) NOT NULL DEFAULT '1' COMMENT 'Stack Size of object to create (-1 = infinite)',
  `palette` tinyint(5) NOT NULL COMMENT 'Palette Color of Object',
  `shade` float NOT NULL COMMENT 'Shade of Object''s Palette',
  `try_To_Bond` bit(1) NOT NULL COMMENT 'Unused?',
  PRIMARY KEY (`id`),
  KEY `wcid_createlist` (`object_Id`),
  CONSTRAINT `wcid_createlist` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=88775 DEFAULT CHARSET=utf8 COMMENT='CreateList Properties of Weenies';
CREATE TABLE `weenie_properties_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyDataId.????)',
  `value` int(10) unsigned NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_did_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_did` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=267013 DEFAULT CHARSET=utf8 COMMENT='DataID Properties of Weenies';
CREATE TABLE `weenie_properties_emote` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `category` int(10) unsigned NOT NULL COMMENT 'EmoteCategory',
  `probability` float NOT NULL DEFAULT '1' COMMENT 'Probability of this EmoteSet being chosen',
  `weenie_Class_Id` int(10) unsigned DEFAULT NULL,
  `style` int(10) unsigned DEFAULT NULL,
  `substyle` int(10) unsigned DEFAULT NULL,
  `quest` text,
  `vendor_Type` int(10) DEFAULT NULL,
  `min_Health` float DEFAULT NULL,
  `max_Health` float DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `wcid_emote` (`object_Id`),
  CONSTRAINT `wcid_emote` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=90717 DEFAULT CHARSET=utf8 COMMENT='Emote Properties of Weenies';
CREATE TABLE `weenie_properties_emote_action` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `emote_Id` int(10) unsigned NOT NULL COMMENT 'Id of the emote this property belongs to',
  `order` int(10) unsigned NOT NULL COMMENT 'Emote Action Sequence Order',
  `type` int(10) unsigned NOT NULL COMMENT 'EmoteType',
  `delay` float NOT NULL DEFAULT '1' COMMENT 'Time to wait before EmoteAction starts execution',
  `extent` float NOT NULL DEFAULT '1' COMMENT '?',
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
  `destination_Type` tinyint(5) DEFAULT NULL COMMENT 'Type of Destination the value applies to (DestinationType.????)',
  `weenie_Class_Id` int(10) unsigned DEFAULT NULL COMMENT 'Weenie Class Id of object to Create',
  `stack_Size` int(10) DEFAULT NULL COMMENT 'Stack Size of object to create (-1 = infinite)',
  `palette` int(10) DEFAULT NULL COMMENT 'Palette Color of Object',
  `shade` float DEFAULT NULL COMMENT 'Shade of Object''s Palette',
  `try_To_Bond` bit(1) DEFAULT NULL COMMENT 'Unused?',
  `obj_Cell_Id` int(10) unsigned DEFAULT NULL,
  `origin_X` float DEFAULT NULL,
  `origin_Y` float DEFAULT NULL,
  `origin_Z` float DEFAULT NULL,
  `angles_W` float DEFAULT NULL,
  `angles_X` float DEFAULT NULL,
  `angles_Y` float DEFAULT NULL,
  `angles_Z` float DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `emoteid_order_uidx` (`emote_Id`,`order`),
  CONSTRAINT `emoteid_emoteaction` FOREIGN KEY (`emote_Id`) REFERENCES `weenie_properties_emote` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=196625 DEFAULT CHARSET=utf8 COMMENT='EmoteAction Properties of Weenies';
CREATE TABLE `weenie_properties_event_filter` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `event` int(10) NOT NULL COMMENT 'Id of Event to filter',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_eventfilter_type_uidx` (`object_Id`,`event`),
  CONSTRAINT `wcid_eventfilter` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=7321 DEFAULT CHARSET=utf8 COMMENT='EventFilter Properties of Weenies';
CREATE TABLE `weenie_properties_float` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyFloat.????)',
  `value` double NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_float_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_float` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=396085 DEFAULT CHARSET=utf8 COMMENT='Float Properties of Weenies';
CREATE TABLE `weenie_properties_generator` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `probability` float NOT NULL DEFAULT '1',
  `weenie_Class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie Class Id of object to generate',
  `delay` float DEFAULT NULL COMMENT 'Amount of delay before generation',
  `init_Create` int(10) NOT NULL DEFAULT '1' COMMENT 'Number of object to generate initially',
  `max_Create` int(10) NOT NULL DEFAULT '1' COMMENT 'Maximum amount of objects to generate',
  `when_Create` int(10) unsigned NOT NULL DEFAULT '2' COMMENT 'When to generate the weenie object',
  `where_Create` int(10) unsigned NOT NULL DEFAULT '4' COMMENT 'Where to generate the weenie object',
  `stack_Size` int(10) DEFAULT NULL COMMENT 'StackSize of object generated',
  `palette_Id` int(10) unsigned DEFAULT NULL COMMENT 'Palette Color of Object Generated',
  `shade` float DEFAULT NULL COMMENT 'Shade of Object generated''s Palette',
  `obj_Cell_Id` int(10) unsigned DEFAULT NULL,
  `origin_X` float DEFAULT NULL,
  `origin_Y` float DEFAULT NULL,
  `origin_Z` float DEFAULT NULL,
  `angles_W` float DEFAULT NULL,
  `angles_X` float DEFAULT NULL,
  `angles_Y` float DEFAULT NULL,
  `angles_Z` float DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `wcid_generator` (`object_Id`),
  CONSTRAINT `wcid_generator` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=28184 DEFAULT CHARSET=utf8 COMMENT='Generator Properties of Weenies';
CREATE TABLE `weenie_properties_i_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInstanceId.????)',
  `value` int(10) unsigned NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_iid_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_iid` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=257 DEFAULT CHARSET=utf8 COMMENT='InstanceID Properties of Weenies';
CREATE TABLE `weenie_properties_int` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInt.????)',
  `value` int(10) NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_int_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_int` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=606854 DEFAULT CHARSET=utf8 COMMENT='Int Properties of Weenies';
CREATE TABLE `weenie_properties_int64` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInt64.????)',
  `value` bigint(10) NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_int64_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_int64` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=355 DEFAULT CHARSET=utf8 COMMENT='Int64 Properties of Weenies';
CREATE TABLE `weenie_properties_palette` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `sub_Palette_Id` int(10) unsigned NOT NULL,
  `offset` smallint(5) unsigned NOT NULL,
  `length` smallint(5) unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_subPaletteId_offset_length_uidx` (`object_Id`,`sub_Palette_Id`,`offset`,`length`),
  CONSTRAINT `wcid_palette` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=93 DEFAULT CHARSET=utf8 COMMENT='Palette Changes (from PCAPs) of Weenies';
CREATE TABLE `weenie_properties_position` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Position',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `position_Type` smallint(5) unsigned NOT NULL COMMENT 'Type of Position the value applies to (PositionType.????)',
  `obj_Cell_Id` int(10) unsigned NOT NULL,
  `origin_X` float NOT NULL,
  `origin_Y` float NOT NULL,
  `origin_Z` float NOT NULL,
  `angles_W` float NOT NULL,
  `angles_X` float NOT NULL,
  `angles_Y` float NOT NULL,
  `angles_Z` float NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_position_type_uidx` (`object_Id`,`position_Type`),
  CONSTRAINT `wcid_position` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=3597 DEFAULT CHARSET=utf8 COMMENT='Position Properties of Weenies';
CREATE TABLE `weenie_properties_skill` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertySkill.????)',
  `level_From_P_P` smallint(5) unsigned NOT NULL COMMENT 'points raised',
  `s_a_c` int(10) unsigned NOT NULL COMMENT 'skill state',
  `p_p` int(10) unsigned NOT NULL COMMENT 'XP spent on this skill',
  `init_Level` int(10) unsigned NOT NULL COMMENT 'starting point for advancement of the skill (eg bonus points)',
  `resistance_At_Last_Check` int(10) unsigned NOT NULL COMMENT 'last use difficulty',
  `last_Used_Time` double NOT NULL COMMENT 'time skill was last used',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_skill_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_skill` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=70248 DEFAULT CHARSET=utf8 COMMENT='Skill Properties of Weenies';
CREATE TABLE `weenie_properties_spell_book` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `spell` int(10) NOT NULL COMMENT 'Id of Spell',
  `probability` float NOT NULL DEFAULT '2' COMMENT 'Chance to cast this spell',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_spellbook_type_uidx` (`object_Id`,`spell`),
  CONSTRAINT `wcid_spellbook` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=59518 DEFAULT CHARSET=utf8 COMMENT='SpellBook Properties of Weenies';
CREATE TABLE `weenie_properties_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint(5) unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyString.????)',
  `value` text NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_string_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_string` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=95168 DEFAULT CHARSET=utf8 COMMENT='String Properties of Weenies';
CREATE TABLE `weenie_properties_texture_map` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int(10) unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `index` tinyint(3) unsigned NOT NULL,
  `old_Id` int(10) unsigned NOT NULL,
  `new_Id` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_index_oldId_uidx` (`object_Id`,`index`,`old_Id`),
  CONSTRAINT `wcid_texturemap` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB AUTO_INCREMENT=158 DEFAULT CHARSET=utf8 COMMENT='Texture Map Changes (from PCAPs) of Weenies';

INSERT INTO `weenie` VALUES (100,'testdrudge',10,'2021-11-01 00:00:00'),(200,'testsword',6,'2021-11-01 00:00:00');
INSERT INTO `weenie_properties_int` VALUES (1,100,1,16),(2,100,25,5),(3,200,1,1),(4,200,5,100);
INSERT INTO `weenie_properties_string` VALUES (1,100,1,'Test Drudge'),(2,200,1,'Test Sword');
INSERT INTO `weenie_properties_float` VALUES (1,200,39,1.5);
INSERT INTO `weenie_properties_emote` VALUES (7,100,1,1,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO `weenie_properties_emote_action` VALUES (11,7,0,10,0,1,NULL,'Hello',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL),(12,7,1,10,0,1,NULL,'Bye',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES (2056994817,100,2847146009,10,20,30,1,0,0,0,' ','2021-11-01 00:00:00'),(2056994818,200,2847146009,11,21,31,1,0,0,0,'','2021-11-01 00:00:00'),(1879076865,200,459008,1,2,3,1,0,0,0,' ','2021-11-01 00:00:00');
INSERT INTO `landblock_instance_link` VALUES (1,2056994817,2056994818,'2021-11-01 00:00:00');
INSERT INTO `encounter` VALUES (1,43444,100,1,2,'2021-11-01 00:00:00');
INSERT INTO `quest` VALUES (1,'TestQuest',3600,-1,NULL,'2021-11-01 00:00:00');
INSERT INTO `recipe` VALUES (5,0,0,0,0,300,1,'ok',0,0,'fail',1,1,NULL,0,0,NULL,0,0,NULL,0,0,NULL,0,'2021-11-01 00:00:00');
INSERT INTO `recipe_mod` VALUES (3,5,'',0,0,0,' ',0,0,0);
INSERT INTO `recipe_mods_int` VALUES (4,3,0,1,2,3,4);
INSERT INTO `cook_book` VALUES (9,5,100,200,'2021-11-01 00:00:00');
INSERT INTO `points_of_interest` VALUES (1,'Holtburg',200,'2021-11-01 00:00:00');
INSERT INTO `version` VALUES (1,'ACE-World-Test','v0.0.1','2021-11-01 00:00:00');
