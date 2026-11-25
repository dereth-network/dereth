/* Vermin (look 6): an AC2 Vermin as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:vermin6}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:vermin6}}, 'ac2-vermin6', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin6}},   1,         16) /* ItemType - Creature */
     , ({{wcid:vermin6}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:vermin6}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:vermin6}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:vermin6}},  16,          1) /* ItemUseable - No */
     , ({{wcid:vermin6}},  25,          5) /* Level */
     , ({{wcid:vermin6}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:vermin6}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:vermin6}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:vermin6}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:vermin6}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:vermin6}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin6}},   1, True ) /* Stuck */
     , ({{wcid:vermin6}},  11, False) /* IgnoreCollisions */
     , ({{wcid:vermin6}},  12, True ) /* ReportCollisions */
     , ({{wcid:vermin6}},  13, False) /* Ethereal */
     , ({{wcid:vermin6}},  14, True ) /* GravityStatus */
     , ({{wcid:vermin6}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin6}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:vermin6}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:vermin6}},   3,    0.5) /* HealthRate */
     , ({{wcid:vermin6}},   4,      5) /* StaminaRate */
     , ({{wcid:vermin6}},   5,      1) /* ManaRate */
     , ({{wcid:vermin6}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:vermin6}},  34,      1) /* PowerupTime */
     , ({{wcid:vermin6}},  36,      1) /* ChargeSpeed */
     , ({{wcid:vermin6}},  39,   1.15) /* DefaultScale */
     , ({{wcid:vermin6}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:vermin6}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:vermin6}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:vermin6}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:vermin6}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:vermin6}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:vermin6}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:vermin6}},  64,      1) /* ResistSlash */
     , ({{wcid:vermin6}},  65,      1) /* ResistPierce */
     , ({{wcid:vermin6}},  66,      1) /* ResistBludgeon */
     , ({{wcid:vermin6}},  67,      1) /* ResistFire */
     , ({{wcid:vermin6}},  68,      1) /* ResistCold */
     , ({{wcid:vermin6}},  69,      1) /* ResistAcid */
     , ({{wcid:vermin6}},  70,      1) /* ResistElectric */
     , ({{wcid:vermin6}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:vermin6}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:vermin6}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:vermin6}},  74,      1) /* ResistManaDrain */
     , ({{wcid:vermin6}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin6}}, 1, 'Vermin (look 6)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin6}},   1, 0x0200EB00) /* Setup */
     , ({{wcid:vermin6}},   2, 0x0900EB00) /* MotionTable */
     , ({{wcid:vermin6}},   4, 0x3000EB00) /* CombatTable */
     , ({{wcid:vermin6}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:vermin6}}, 0, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 0, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 1, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 1, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 2, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 2, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 3, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 4, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 5, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 6, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 7, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 7, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 8, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 9, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 10, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 11, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 12, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 12, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 13, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 13, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 14, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 14, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 15, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 15, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 16, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 16, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 17, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 19, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 20, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 21, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 21, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 22, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 23, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 24, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 25, 0x0500EB00, 0x0500EB07)
     , ({{wcid:vermin6}}, 25, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 26, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 27, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 28, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 29, 0x0500EB01, 0x0500EB07)
     , ({{wcid:vermin6}}, 30, 0x0500EB01, 0x0500EB07);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:vermin6}}, 1, 40, 0, 0)
     , ({{wcid:vermin6}}, 2, 40, 0, 0)
     , ({{wcid:vermin6}}, 3, 40, 0, 0)
     , ({{wcid:vermin6}}, 4, 40, 0, 0)
     , ({{wcid:vermin6}}, 5, 10, 0, 0)
     , ({{wcid:vermin6}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:vermin6}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:vermin6}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:vermin6}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:vermin6}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:vermin6}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:vermin6}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:vermin6}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:vermin6}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:vermin6}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:vermin6}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:vermin6}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:vermin6}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:vermin6}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:vermin6}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:vermin6}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
