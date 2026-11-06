


CREATE TABLE `version` (
  `id` int unsigned NOT NULL AUTO_INCREMENT,
  `base_Version` varchar(45) DEFAULT NULL,
  `patch_Version` varchar(45) DEFAULT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Version Information';


CREATE TABLE `weenie` (
  `class_Id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Weenie Class Id (wcid) / (WCID) / (weenieClassId)',
  `class_Name` varchar(100) NOT NULL COMMENT 'Weenie Class Name (W_????_CLASS)',
  `type` int NOT NULL DEFAULT '0' COMMENT 'WeenieType',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`class_Id`),
  UNIQUE KEY `className_UNIQUE` (`class_Name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Weenies';


CREATE TABLE `weenie_properties_anim_part` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `index` tinyint unsigned NOT NULL,
  `animation_Id` int unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_index_uidx` (`object_Id`,`index`),
  CONSTRAINT `wcid_animpart` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Animation Part Changes (from PCAPs) of Weenies';


CREATE TABLE `weenie_properties_attribute` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyAttribute.????)',
  `init_Level` int unsigned NOT NULL COMMENT 'innate points',
  `level_From_C_P` int unsigned NOT NULL COMMENT 'points raised',
  `c_P_Spent` int unsigned NOT NULL COMMENT 'XP spent on this attribute',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_attribute_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_attribute` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Attribute Properties of Weenies';


CREATE TABLE `weenie_properties_attribute_2nd` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyAttribute2nd.????)',
  `init_Level` int unsigned NOT NULL COMMENT 'innate points',
  `level_From_C_P` int unsigned NOT NULL COMMENT 'points raised',
  `c_P_Spent` int unsigned NOT NULL COMMENT 'XP spent on this attribute',
  `current_Level` int unsigned NOT NULL COMMENT 'current value of the vital',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_attribute2nd_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_attribute2nd` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Attribute2nd (Vital) Properties of Weenies';


CREATE TABLE `weenie_properties_body_part` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `key` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertySkill.????)',
  `d_Type` int NOT NULL,
  `d_Val` int NOT NULL,
  `d_Var` float NOT NULL,
  `base_Armor` int NOT NULL,
  `armor_Vs_Slash` int NOT NULL,
  `armor_Vs_Pierce` int NOT NULL,
  `armor_Vs_Bludgeon` int NOT NULL,
  `armor_Vs_Cold` int NOT NULL,
  `armor_Vs_Fire` int NOT NULL,
  `armor_Vs_Acid` int NOT NULL,
  `armor_Vs_Electric` int NOT NULL,
  `armor_Vs_Nether` int NOT NULL,
  `b_h` int NOT NULL,
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
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Body Part Properties of Weenies';


CREATE TABLE `weenie_properties_book` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `max_Num_Pages` int NOT NULL DEFAULT '1' COMMENT 'Maximum number of pages per book',
  `max_Num_Chars_Per_Page` int NOT NULL DEFAULT '1000' COMMENT 'Maximum number of characters per page',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_bookdata_uidx` (`object_Id`),
  CONSTRAINT `wcid_bookdata` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Book Properties of Weenies';


CREATE TABLE `weenie_properties_book_page_data` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the Book object this page belongs to',
  `page_Id` int unsigned NOT NULL COMMENT 'Id of the page number for this page',
  `author_Id` int unsigned NOT NULL COMMENT 'Id of the Author of this page',
  `author_Name` varchar(255) NOT NULL DEFAULT '' COMMENT 'Character Name of the Author of this page',
  `author_Account` varchar(255) NOT NULL DEFAULT 'prewritten' COMMENT 'Account Name of the Author of this page',
  `ignore_Author` bit(1) NOT NULL COMMENT 'if this is true, any character in the world can change the page',
  `page_Text` text NOT NULL COMMENT 'Text of the Page',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_pageid_uidx` (`object_Id`,`page_Id`),
  CONSTRAINT `wcid_pagedata` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Page Properties of Weenies';


CREATE TABLE `weenie_properties_bool` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyBool.????)',
  `value` bit(1) NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_bool_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_bool` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Bool Properties of Weenies';


CREATE TABLE `weenie_properties_create_list` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `destination_Type` tinyint NOT NULL COMMENT 'Type of Destination the value applies to (DestinationType.????)',
  `weenie_Class_Id` int unsigned NOT NULL COMMENT 'Weenie Class Id of object to Create',
  `stack_Size` int NOT NULL DEFAULT '1' COMMENT 'Stack Size of object to create (-1 = infinite)',
  `palette` tinyint NOT NULL COMMENT 'Palette Color of Object',
  `shade` float NOT NULL COMMENT 'Shade of Object''s Palette',
  `try_To_Bond` bit(1) NOT NULL COMMENT 'Unused?',
  PRIMARY KEY (`id`),
  KEY `wcid_createlist` (`object_Id`),
  CONSTRAINT `wcid_createlist` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='CreateList Properties of Weenies';


CREATE TABLE `weenie_properties_d_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyDataId.????)',
  `value` int unsigned NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_did_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_did` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='DataID Properties of Weenies';


CREATE TABLE `weenie_properties_emote` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `category` int unsigned NOT NULL COMMENT 'EmoteCategory',
  `probability` float NOT NULL DEFAULT '1' COMMENT 'Probability of this EmoteSet being chosen',
  `weenie_Class_Id` int unsigned DEFAULT NULL,
  `style` int unsigned DEFAULT NULL,
  `substyle` int unsigned DEFAULT NULL,
  `quest` text,
  `vendor_Type` int DEFAULT NULL,
  `min_Health` float DEFAULT NULL,
  `max_Health` float DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `wcid_emote` (`object_Id`),
  CONSTRAINT `wcid_emote` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Emote Properties of Weenies';


CREATE TABLE `weenie_properties_emote_action` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `emote_Id` int unsigned NOT NULL COMMENT 'Id of the emote this property belongs to',
  `order` int unsigned NOT NULL COMMENT 'Emote Action Sequence Order',
  `type` int unsigned NOT NULL COMMENT 'EmoteType',
  `delay` float NOT NULL DEFAULT '1' COMMENT 'Time to wait before EmoteAction starts execution',
  `extent` float NOT NULL DEFAULT '1' COMMENT '?',
  `motion` int unsigned DEFAULT NULL COMMENT 'MotionCommand -- a uint32; the high bit marks a stance',
  `message` text,
  `test_String` text,
  `min` int DEFAULT NULL,
  `max` int DEFAULT NULL,
  `min_64` bigint DEFAULT NULL,
  `max_64` bigint DEFAULT NULL,
  `min_Dbl` double DEFAULT NULL,
  `max_Dbl` double DEFAULT NULL,
  `stat` int DEFAULT NULL,
  `display` bit(1) DEFAULT NULL,
  `amount` int DEFAULT NULL,
  `amount_64` bigint DEFAULT NULL,
  `hero_X_P_64` bigint DEFAULT NULL,
  `percent` double DEFAULT NULL,
  `spell_Id` int DEFAULT NULL,
  `wealth_Rating` int DEFAULT NULL,
  `treasure_Class` int DEFAULT NULL,
  `treasure_Type` int DEFAULT NULL,
  `p_Script` int DEFAULT NULL,
  `sound` int DEFAULT NULL,
  `destination_Type` tinyint DEFAULT NULL COMMENT 'Type of Destination the value applies to (DestinationType.????)',
  `weenie_Class_Id` int unsigned DEFAULT NULL COMMENT 'Weenie Class Id of object to Create',
  `stack_Size` int DEFAULT NULL COMMENT 'Stack Size of object to create (-1 = infinite)',
  `palette` int DEFAULT NULL COMMENT 'Palette Color of Object',
  `shade` float DEFAULT NULL COMMENT 'Shade of Object''s Palette',
  `try_To_Bond` bit(1) DEFAULT NULL COMMENT 'Unused?',
  `obj_Cell_Id` int unsigned DEFAULT NULL,
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
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='EmoteAction Properties of Weenies';


CREATE TABLE `weenie_properties_event_filter` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `event` int NOT NULL COMMENT 'Id of Event to filter',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_eventfilter_type_uidx` (`object_Id`,`event`),
  CONSTRAINT `wcid_eventfilter` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='EventFilter Properties of Weenies';


CREATE TABLE `weenie_properties_float` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyFloat.????)',
  `value` double NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_float_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_float` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Float Properties of Weenies';


CREATE TABLE `weenie_properties_generator` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `probability` float NOT NULL DEFAULT '1',
  `weenie_Class_Id` int unsigned NOT NULL COMMENT 'Weenie Class Id of object to generate',
  `delay` float DEFAULT NULL COMMENT 'Amount of delay before generation',
  `init_Create` int NOT NULL DEFAULT '1' COMMENT 'Number of object to generate initially',
  `max_Create` int NOT NULL DEFAULT '1' COMMENT 'Maximum amount of objects to generate',
  `when_Create` int unsigned NOT NULL DEFAULT '2' COMMENT 'When to generate the weenie object',
  `where_Create` int unsigned NOT NULL DEFAULT '4' COMMENT 'Where to generate the weenie object',
  `stack_Size` int DEFAULT NULL COMMENT 'StackSize of object generated',
  `palette_Id` int unsigned DEFAULT NULL COMMENT 'Palette Color of Object Generated',
  `shade` float DEFAULT NULL COMMENT 'Shade of Object generated''s Palette',
  `obj_Cell_Id` int unsigned DEFAULT NULL,
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
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Generator Properties of Weenies';


CREATE TABLE `weenie_properties_i_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInstanceId.????)',
  `value` int unsigned NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_iid_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_iid` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='InstanceID Properties of Weenies';


CREATE TABLE `weenie_properties_int` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInt.????)',
  `value` int NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_int_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_int` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Int Properties of Weenies';


CREATE TABLE `weenie_properties_int64` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyInt64.????)',
  `value` bigint NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_int64_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_int64` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Int64 Properties of Weenies';


CREATE TABLE `weenie_properties_palette` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `sub_Palette_Id` int unsigned NOT NULL,
  `offset` smallint unsigned NOT NULL,
  `length` smallint unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_subPaletteId_offset_length_uidx` (`object_Id`,`sub_Palette_Id`,`offset`,`length`),
  CONSTRAINT `wcid_palette` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Palette Changes (from PCAPs) of Weenies';


CREATE TABLE `weenie_properties_position` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Position',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `position_Type` smallint unsigned NOT NULL COMMENT 'Type of Position the value applies to (PositionType.????)',
  `obj_Cell_Id` int unsigned NOT NULL,
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
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Position Properties of Weenies';


CREATE TABLE `weenie_properties_skill` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertySkill.????)',
  `level_From_P_P` smallint unsigned NOT NULL COMMENT 'points raised',
  `s_a_c` int unsigned NOT NULL COMMENT 'skill state',
  `p_p` int unsigned NOT NULL COMMENT 'XP spent on this skill',
  `init_Level` int unsigned NOT NULL COMMENT 'starting point for advancement of the skill (eg bonus points)',
  `resistance_At_Last_Check` int unsigned NOT NULL COMMENT 'last use difficulty',
  `last_Used_Time` double NOT NULL COMMENT 'time skill was last used',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_skill_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_skill` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Skill Properties of Weenies';


CREATE TABLE `weenie_properties_spell_book` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `spell` int NOT NULL COMMENT 'Id of Spell',
  `probability` float NOT NULL DEFAULT '2' COMMENT 'Chance to cast this spell',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_spellbook_type_uidx` (`object_Id`,`spell`),
  CONSTRAINT `wcid_spellbook` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='SpellBook Properties of Weenies';


CREATE TABLE `weenie_properties_string` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `type` smallint unsigned NOT NULL COMMENT 'Type of Property the value applies to (PropertyString.????)',
  `value` text NOT NULL COMMENT 'Value of this Property',
  PRIMARY KEY (`id`),
  UNIQUE KEY `wcid_string_type_uidx` (`object_Id`,`type`),
  CONSTRAINT `wcid_string` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='String Properties of Weenies';


CREATE TABLE `weenie_properties_texture_map` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Property',
  `object_Id` int unsigned NOT NULL COMMENT 'Id of the object this property belongs to',
  `index` tinyint unsigned NOT NULL,
  `old_Id` int unsigned NOT NULL,
  `new_Id` int unsigned NOT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `object_Id_index_oldId_uidx` (`object_Id`,`index`,`old_Id`),
  CONSTRAINT `wcid_texturemap` FOREIGN KEY (`object_Id`) REFERENCES `weenie` (`class_Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Texture Map Changes (from PCAPs) of Weenies';


CREATE TABLE `landblock_instance` (
  `guid` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Instance',
  `landblock` int GENERATED ALWAYS AS ((`obj_Cell_Id` >> 16)) VIRTUAL,
  `weenie_Class_Id` int unsigned NOT NULL COMMENT 'Weenie Class Id of object to spawn',
  `obj_Cell_Id` int unsigned NOT NULL,
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
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Weenie Instances for each Landblock';


CREATE TABLE `landblock_instance_link` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Instance Link',
  `parent_GUID` int unsigned NOT NULL COMMENT 'GUID of parent instance',
  `child_GUID` int unsigned NOT NULL COMMENT 'GUID of child instance',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `parent_child_uidx` (`parent_GUID`,`child_GUID`),
  KEY `child_idx` (`child_GUID`),
  CONSTRAINT `instance_link` FOREIGN KEY (`parent_GUID`) REFERENCES `landblock_instance` (`guid`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Weenie Instance Links';


CREATE TABLE `encounter` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Encounter',
  `landblock` int NOT NULL COMMENT 'Landblock for this Encounter',
  `weenie_Class_Id` int unsigned NOT NULL COMMENT 'Weenie Class Id of generator/object to spawn for Encounter',
  `cell_X` int NOT NULL COMMENT 'CellX position of this Encounter',
  `cell_Y` int NOT NULL COMMENT 'CellY position of this Encounter',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `landblock_cellx_celly_uidx` (`landblock`,`cell_X`,`cell_Y`),
  KEY `landblock_idx` (`landblock`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Encounters';


CREATE TABLE `quest` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Quest',
  `name` varchar(255) NOT NULL COMMENT 'Unique Name of Quest',
  `min_Delta` int NOT NULL COMMENT 'Minimum time between Quest completions',
  `max_Solves` int NOT NULL COMMENT 'Maximum number of times Quest can be completed',
  `message` text COMMENT 'Quest solved text - unused?',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `name_UNIQUE` (`name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Quests';


CREATE TABLE `event` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Event',
  `name` varchar(255) NOT NULL COMMENT 'Unique Event of Quest',
  `start_Time` int NOT NULL DEFAULT '-1' COMMENT 'Unixtime of Event Start',
  `end_Time` int NOT NULL DEFAULT '-1' COMMENT 'Unixtime of Event End',
  `state` int NOT NULL COMMENT 'State of Event (GameEventState)',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `name_UNIQUE` (`name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Events';



CREATE TABLE `recipe` (
  `id` int unsigned NOT NULL COMMENT 'Unique Id of this Recipe',
  `unknown_1` int unsigned NOT NULL,
  `skill` int unsigned NOT NULL,
  `difficulty` int unsigned NOT NULL,
  `salvage_Type` int unsigned NOT NULL,
  `success_W_C_I_D` int unsigned NOT NULL COMMENT 'Weenie Class Id of object to create upon successful application of this recipe',
  `success_Amount` int unsigned NOT NULL COMMENT 'Amount of objects to create upon successful application of this recipe',
  `success_Message` text,
  `fail_W_C_I_D` int unsigned NOT NULL COMMENT 'Weenie Class Id of object to create upon failing application of this recipe',
  `fail_Amount` int unsigned NOT NULL COMMENT 'Amount of objects to create upon failing application of this recipe',
  `fail_Message` text,
  `success_Destroy_Source_Chance` double NOT NULL,
  `success_Destroy_Source_Amount` int unsigned NOT NULL,
  `success_Destroy_Source_Message` text,
  `success_Destroy_Target_Chance` double NOT NULL,
  `success_Destroy_Target_Amount` int unsigned NOT NULL,
  `success_Destroy_Target_Message` text,
  `fail_Destroy_Source_Chance` double NOT NULL,
  `fail_Destroy_Source_Amount` int unsigned NOT NULL,
  `fail_Destroy_Source_Message` text,
  `fail_Destroy_Target_Chance` double NOT NULL,
  `fail_Destroy_Target_Amount` int unsigned NOT NULL,
  `fail_Destroy_Target_Message` text,
  `data_Id` int unsigned NOT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipes';


CREATE TABLE `cook_book` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this cook book instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `source_W_C_I_D` int unsigned NOT NULL COMMENT 'Weenie Class Id of the source object for this recipe',
  `target_W_C_I_D` int unsigned NOT NULL COMMENT 'Weenie Class Id of the target object for this recipe',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `recipe_source_target_uidx` (`recipe_Id`,`source_W_C_I_D`,`target_W_C_I_D`),
  KEY `source_idx` (`source_W_C_I_D`),
  KEY `target_idx` (`target_W_C_I_D`),
  CONSTRAINT `cookbook_recipe` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Cook Book for Recipes';


CREATE TABLE `recipe_mod` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL DEFAULT 0 COMMENT 'Which of the 8 mod blocks this is -- OUR COLUMN, see the note above',
  `requires` tinyint NOT NULL DEFAULT 0 COMMENT 'OUR COLUMN: which Modify* the player must be able to pay. Bit 0 health, 1 stamina, 2 mana',
  `executes_On_Success` bit(1) NOT NULL,
  `health` int NOT NULL,
  `stamina` int NOT NULL,
  `mana` int NOT NULL,
  `unknown_7` bit(1) NOT NULL,
  `data_Id` int NOT NULL,
  `unknown_9` int NOT NULL,
  `instance_Id` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_Mod` (`recipe_Id`),
  CONSTRAINT `recipeId_Mod` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Mods';


CREATE TABLE `recipe_mods_bool` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_bool` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_bool` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Bool Mods';


CREATE TABLE `recipe_mods_d_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int unsigned NOT NULL,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_did` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_did` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe DID Mods';


CREATE TABLE `recipe_mods_float` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` double NOT NULL,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_float` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_float` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Float Mods';


CREATE TABLE `recipe_mods_i_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int unsigned NOT NULL,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_iid` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_iid` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe IID Mods';


CREATE TABLE `recipe_mods_int` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int NOT NULL,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_int` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_int` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Int Mods';


CREATE TABLE `recipe_mods_string` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Mod instance',
  `recipe_Mod_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe Mod',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` text,
  `enum` int NOT NULL,
  `source` int NOT NULL,
  PRIMARY KEY (`id`),
  KEY `recipeId_mod_string` (`recipe_Mod_Id`),
  CONSTRAINT `recipeId_mod_string` FOREIGN KEY (`recipe_Mod_Id`) REFERENCES `recipe_mod` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe String Mods';


CREATE TABLE `recipe_requirements_bool` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` bit(1) NOT NULL,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_bool` (`recipe_Id`),
  CONSTRAINT `recipeId_req_bool` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Bool Requirments';


CREATE TABLE `recipe_requirements_d_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int unsigned NOT NULL,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_did` (`recipe_Id`),
  CONSTRAINT `recipeId_req_did` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe DID Requirments';


CREATE TABLE `recipe_requirements_float` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` double NOT NULL,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_float` (`recipe_Id`),
  CONSTRAINT `recipeId_req_float` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Float Requirments';


CREATE TABLE `recipe_requirements_i_i_d` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int unsigned NOT NULL,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_iid` (`recipe_Id`),
  CONSTRAINT `recipeId_req_iid` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe IID Requirments';


CREATE TABLE `recipe_requirements_int` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` int NOT NULL,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_int` (`recipe_Id`),
  CONSTRAINT `recipeId_req_int` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe Int Requirments';


CREATE TABLE `recipe_requirements_string` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Recipe Requirement instance',
  `recipe_Id` int unsigned NOT NULL COMMENT 'Unique Id of Recipe',
  `index` tinyint NOT NULL,
  `stat` int NOT NULL,
  `value` text,
  `enum` int NOT NULL,
  `message` text,
  PRIMARY KEY (`id`),
  KEY `recipeId_req_string` (`recipe_Id`),
  CONSTRAINT `recipeId_req_string` FOREIGN KEY (`recipe_Id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Recipe String Requirments';



CREATE TABLE `spell` (
  `id` int unsigned NOT NULL COMMENT 'Unique Id of this Spell',
  `name` text NOT NULL,
  `stat_Mod_Type` int unsigned DEFAULT NULL,
  `stat_Mod_Key` int unsigned DEFAULT NULL,
  `stat_Mod_Val` float DEFAULT NULL,
  `e_Type` int unsigned DEFAULT NULL,
  `base_Intensity` int DEFAULT NULL,
  `variance` int DEFAULT NULL,
  `wcid` int unsigned DEFAULT NULL,
  `num_Projectiles` int DEFAULT NULL,
  `num_Projectiles_Variance` int DEFAULT NULL,
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
  `imbued_Effect` int unsigned DEFAULT NULL,
  `slayer_Creature_Type` int DEFAULT NULL,
  `slayer_Damage_Bonus` float DEFAULT NULL,
  `crit_Freq` double DEFAULT NULL,
  `crit_Multiplier` double DEFAULT NULL,
  `ignore_Magic_Resist` int DEFAULT NULL,
  `elemental_Modifier` double DEFAULT NULL,
  `drain_Percentage` float DEFAULT NULL,
  `damage_Ratio` float DEFAULT NULL,
  `damage_Type` int DEFAULT NULL,
  `boost` int DEFAULT NULL,
  `boost_Variance` int DEFAULT NULL,
  `source` int DEFAULT NULL,
  `destination` int DEFAULT NULL,
  `proportion` float DEFAULT NULL,
  `loss_Percent` float DEFAULT NULL,
  `source_Loss` int DEFAULT NULL,
  `transfer_Cap` int DEFAULT NULL,
  `max_Boost_Allowed` int DEFAULT NULL,
  `transfer_Bitfield` int unsigned DEFAULT NULL,
  `index` int DEFAULT NULL,
  `link` int DEFAULT NULL,
  `position_Obj_Cell_ID` int unsigned DEFAULT NULL,
  `position_Origin_X` float DEFAULT NULL,
  `position_Origin_Y` float DEFAULT NULL,
  `position_Origin_Z` float DEFAULT NULL,
  `position_Angles_W` float DEFAULT NULL,
  `position_Angles_X` float DEFAULT NULL,
  `position_Angles_Y` float DEFAULT NULL,
  `position_Angles_Z` float DEFAULT NULL,
  `min_Power` int DEFAULT NULL,
  `max_Power` int DEFAULT NULL,
  `power_Variance` float DEFAULT NULL,
  `dispel_School` int DEFAULT NULL,
  `align` int DEFAULT NULL,
  `number` int DEFAULT NULL,
  `number_Variance` float DEFAULT NULL,
  `dot_Duration` double DEFAULT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Spell Table Extended Data';



CREATE TABLE `treasure_death` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Treasure',
  `treasure_Type` int unsigned NOT NULL COMMENT 'Type of Treasure for this instance',
  `tier` int NOT NULL,
  `loot_Quality_Mod` float NOT NULL COMMENT '0..1, position inside the tier',
  `weapon_Chance` int NOT NULL,
  `weapon_Min_Amount` int NOT NULL,
  `weapon_Max_Amount` int NOT NULL,
  `armor_Chance` int NOT NULL,
  `armor_Min_Amount` int NOT NULL,
  `armor_Max_Amount` int NOT NULL,
  `item_Chance` int NOT NULL,
  `item_Min_Amount` int NOT NULL,
  `item_Max_Amount` int NOT NULL,
  `magic_Item_Chance` int NOT NULL,
  `magic_Item_Min_Amount` int NOT NULL,
  `magic_Item_Max_Amount` int NOT NULL,
  `consumable_Chance` int NOT NULL,
  `consumable_Min_Amount` int NOT NULL,
  `consumable_Max_Amount` int NOT NULL,
  `valuable_Chance` int NOT NULL,
  `valuable_Min_Amount` int NOT NULL,
  `valuable_Max_Amount` int NOT NULL,
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `treasureType_idx` (`treasure_Type`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Death Treasure: six slot groups, one per 1999 appraisal skill plus consumables/valuables. Ours, not ACE''s shape.';


CREATE TABLE `treasure_wielded` (
  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id of this Treasure',
  `treasure_Type` int unsigned NOT NULL COMMENT 'Type of Treasure for this instance',
  `weenie_Class_Id` int unsigned NOT NULL COMMENT 'Weenie Class Id of Treasure to Generate',
  `palette_Id` int unsigned NOT NULL COMMENT 'Palette Color of Object Generated',
  `unknown_1` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `shade` float NOT NULL COMMENT 'Shade of Object generated''s Palette',
  `stack_Size` int NOT NULL DEFAULT '1' COMMENT 'Stack Size of object to create (-1 = infinite)',
  `stack_Size_Variance` float NOT NULL,
  `probability` float NOT NULL,
  `unknown_3` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_4` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_5` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `set_Start` bit(1) NOT NULL,
  `has_Sub_Set` bit(1) NOT NULL,
  `continues_Previous_Set` bit(1) NOT NULL,
  `unknown_9` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_10` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_11` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `unknown_12` int unsigned NOT NULL COMMENT 'Always 0 in cache.bin',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `treasureType_idx` (`treasure_Type`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci COMMENT='Wielded Treasure';
