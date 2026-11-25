/* Mimbu (look 4): an AC2 Mimbu as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:mimbu4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:mimbu4}}, 'ac2-mimbu4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mimbu4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:mimbu4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:mimbu4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:mimbu4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:mimbu4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:mimbu4}},  25,          5) /* Level */
     , ({{wcid:mimbu4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:mimbu4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:mimbu4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:mimbu4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:mimbu4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:mimbu4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mimbu4}},   1, True ) /* Stuck */
     , ({{wcid:mimbu4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:mimbu4}},  12, True ) /* ReportCollisions */
     , ({{wcid:mimbu4}},  13, False) /* Ethereal */
     , ({{wcid:mimbu4}},  14, True ) /* GravityStatus */
     , ({{wcid:mimbu4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mimbu4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:mimbu4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:mimbu4}},   3,    0.5) /* HealthRate */
     , ({{wcid:mimbu4}},   4,      5) /* StaminaRate */
     , ({{wcid:mimbu4}},   5,      1) /* ManaRate */
     , ({{wcid:mimbu4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:mimbu4}},  34,      1) /* PowerupTime */
     , ({{wcid:mimbu4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:mimbu4}},  39,    1.0) /* DefaultScale */
     , ({{wcid:mimbu4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:mimbu4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:mimbu4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:mimbu4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:mimbu4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:mimbu4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:mimbu4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:mimbu4}},  64,      1) /* ResistSlash */
     , ({{wcid:mimbu4}},  65,      1) /* ResistPierce */
     , ({{wcid:mimbu4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:mimbu4}},  67,      1) /* ResistFire */
     , ({{wcid:mimbu4}},  68,      1) /* ResistCold */
     , ({{wcid:mimbu4}},  69,      1) /* ResistAcid */
     , ({{wcid:mimbu4}},  70,      1) /* ResistElectric */
     , ({{wcid:mimbu4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:mimbu4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:mimbu4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:mimbu4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:mimbu4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mimbu4}}, 1, 'Mimbu (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mimbu4}},   1, 0x0200E700) /* Setup */
     , ({{wcid:mimbu4}},   2, 0x0900E700) /* MotionTable */
     , ({{wcid:mimbu4}},   4, 0x3000E700) /* CombatTable */
     , ({{wcid:mimbu4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:mimbu4}}, 0, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 1, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 2, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 3, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 4, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 5, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 7, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 8, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 9, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 10, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 11, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 12, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 13, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 14, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 15, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 16, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 17, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 18, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 20, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 21, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 22, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 23, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 24, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 25, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 26, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 27, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 28, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 29, 0x0500E702, 0x0500E705)
     , ({{wcid:mimbu4}}, 30, 0x0500E702, 0x0500E705);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:mimbu4}}, 1, 40, 0, 0)
     , ({{wcid:mimbu4}}, 2, 40, 0, 0)
     , ({{wcid:mimbu4}}, 3, 40, 0, 0)
     , ({{wcid:mimbu4}}, 4, 40, 0, 0)
     , ({{wcid:mimbu4}}, 5, 10, 0, 0)
     , ({{wcid:mimbu4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:mimbu4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:mimbu4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:mimbu4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:mimbu4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:mimbu4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:mimbu4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:mimbu4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:mimbu4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:mimbu4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:mimbu4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:mimbu4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:mimbu4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:mimbu4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:mimbu4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:mimbu4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
