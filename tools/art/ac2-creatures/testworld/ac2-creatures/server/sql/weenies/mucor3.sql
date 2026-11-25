/* Mucor (look 3): an AC2 Mucor as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:mucor3}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:mucor3}}, 'ac2-mucor3', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor3}},   1,         16) /* ItemType - Creature */
     , ({{wcid:mucor3}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:mucor3}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:mucor3}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:mucor3}},  16,          1) /* ItemUseable - No */
     , ({{wcid:mucor3}},  25,          5) /* Level */
     , ({{wcid:mucor3}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:mucor3}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:mucor3}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:mucor3}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:mucor3}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:mucor3}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor3}},   1, True ) /* Stuck */
     , ({{wcid:mucor3}},  11, False) /* IgnoreCollisions */
     , ({{wcid:mucor3}},  12, True ) /* ReportCollisions */
     , ({{wcid:mucor3}},  13, False) /* Ethereal */
     , ({{wcid:mucor3}},  14, True ) /* GravityStatus */
     , ({{wcid:mucor3}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor3}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:mucor3}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:mucor3}},   3,    0.5) /* HealthRate */
     , ({{wcid:mucor3}},   4,      5) /* StaminaRate */
     , ({{wcid:mucor3}},   5,      1) /* ManaRate */
     , ({{wcid:mucor3}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:mucor3}},  34,      1) /* PowerupTime */
     , ({{wcid:mucor3}},  36,      1) /* ChargeSpeed */
     , ({{wcid:mucor3}},  39,   0.75) /* DefaultScale */
     , ({{wcid:mucor3}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:mucor3}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:mucor3}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:mucor3}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:mucor3}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:mucor3}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:mucor3}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:mucor3}},  64,      1) /* ResistSlash */
     , ({{wcid:mucor3}},  65,      1) /* ResistPierce */
     , ({{wcid:mucor3}},  66,      1) /* ResistBludgeon */
     , ({{wcid:mucor3}},  67,      1) /* ResistFire */
     , ({{wcid:mucor3}},  68,      1) /* ResistCold */
     , ({{wcid:mucor3}},  69,      1) /* ResistAcid */
     , ({{wcid:mucor3}},  70,      1) /* ResistElectric */
     , ({{wcid:mucor3}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:mucor3}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:mucor3}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:mucor3}},  74,      1) /* ResistManaDrain */
     , ({{wcid:mucor3}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor3}}, 1, 'Mucor (look 3)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor3}},   1, 0x0200E800) /* Setup */
     , ({{wcid:mucor3}},   2, 0x0900E800) /* MotionTable */
     , ({{wcid:mucor3}},   4, 0x3000E800) /* CombatTable */
     , ({{wcid:mucor3}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:mucor3}}, 0, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 1, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 2, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 3, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 4, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 5, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 6, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 7, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 8, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 9, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 10, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 11, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 12, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 13, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 14, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 15, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 16, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 17, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 18, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 19, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 20, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 21, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 22, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 23, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 24, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 25, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 26, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 27, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 28, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 29, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 30, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 31, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 32, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 33, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 34, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 35, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 36, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 37, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 38, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 39, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 40, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 41, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 42, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 43, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 44, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 45, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 46, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 47, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 48, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 49, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 50, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 51, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 52, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 53, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 54, 0x0500E800, 0x0500E802)
     , ({{wcid:mucor3}}, 55, 0x0500E800, 0x0500E802);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:mucor3}}, 1, 40, 0, 0)
     , ({{wcid:mucor3}}, 2, 40, 0, 0)
     , ({{wcid:mucor3}}, 3, 40, 0, 0)
     , ({{wcid:mucor3}}, 4, 40, 0, 0)
     , ({{wcid:mucor3}}, 5, 10, 0, 0)
     , ({{wcid:mucor3}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:mucor3}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:mucor3}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:mucor3}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:mucor3}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:mucor3}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:mucor3}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:mucor3}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:mucor3}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:mucor3}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:mucor3}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:mucor3}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:mucor3}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:mucor3}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:mucor3}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:mucor3}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
