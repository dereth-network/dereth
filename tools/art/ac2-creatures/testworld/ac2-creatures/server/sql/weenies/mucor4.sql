/* Mucor (look 4): an AC2 Mucor as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:mucor4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:mucor4}}, 'ac2-mucor4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:mucor4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:mucor4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:mucor4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:mucor4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:mucor4}},  25,          5) /* Level */
     , ({{wcid:mucor4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:mucor4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:mucor4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:mucor4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:mucor4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:mucor4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor4}},   1, True ) /* Stuck */
     , ({{wcid:mucor4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:mucor4}},  12, True ) /* ReportCollisions */
     , ({{wcid:mucor4}},  13, False) /* Ethereal */
     , ({{wcid:mucor4}},  14, True ) /* GravityStatus */
     , ({{wcid:mucor4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:mucor4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:mucor4}},   3,    0.5) /* HealthRate */
     , ({{wcid:mucor4}},   4,      5) /* StaminaRate */
     , ({{wcid:mucor4}},   5,      1) /* ManaRate */
     , ({{wcid:mucor4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:mucor4}},  34,      1) /* PowerupTime */
     , ({{wcid:mucor4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:mucor4}},  39,   0.75) /* DefaultScale */
     , ({{wcid:mucor4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:mucor4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:mucor4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:mucor4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:mucor4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:mucor4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:mucor4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:mucor4}},  64,      1) /* ResistSlash */
     , ({{wcid:mucor4}},  65,      1) /* ResistPierce */
     , ({{wcid:mucor4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:mucor4}},  67,      1) /* ResistFire */
     , ({{wcid:mucor4}},  68,      1) /* ResistCold */
     , ({{wcid:mucor4}},  69,      1) /* ResistAcid */
     , ({{wcid:mucor4}},  70,      1) /* ResistElectric */
     , ({{wcid:mucor4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:mucor4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:mucor4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:mucor4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:mucor4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor4}}, 1, 'Mucor (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor4}},   1, 0x0200E800) /* Setup */
     , ({{wcid:mucor4}},   2, 0x0900E800) /* MotionTable */
     , ({{wcid:mucor4}},   4, 0x3000E800) /* CombatTable */
     , ({{wcid:mucor4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:mucor4}}, 0, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 1, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 2, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 3, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 4, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 5, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 6, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 7, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 8, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 9, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 10, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 11, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 12, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 13, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 14, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 15, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 16, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 17, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 18, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 19, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 20, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 21, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 22, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 23, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 24, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 25, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 26, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 27, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 28, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 29, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 30, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 31, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 32, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 33, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 34, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 35, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 36, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 37, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 38, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 39, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 40, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 41, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 42, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 43, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 44, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 45, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 46, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 47, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 48, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 49, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 50, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 51, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 52, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 53, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 54, 0x0500E800, 0x0500E803)
     , ({{wcid:mucor4}}, 55, 0x0500E800, 0x0500E803);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:mucor4}}, 1, 40, 0, 0)
     , ({{wcid:mucor4}}, 2, 40, 0, 0)
     , ({{wcid:mucor4}}, 3, 40, 0, 0)
     , ({{wcid:mucor4}}, 4, 40, 0, 0)
     , ({{wcid:mucor4}}, 5, 10, 0, 0)
     , ({{wcid:mucor4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:mucor4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:mucor4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:mucor4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:mucor4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:mucor4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:mucor4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:mucor4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:mucor4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:mucor4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:mucor4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:mucor4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:mucor4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:mucor4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:mucor4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:mucor4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
