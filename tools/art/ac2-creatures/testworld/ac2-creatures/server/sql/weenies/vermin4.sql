/* Vermin (look 4): an AC2 Vermin as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:vermin4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:vermin4}}, 'ac2-vermin4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:vermin4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:vermin4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:vermin4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:vermin4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:vermin4}},  25,          5) /* Level */
     , ({{wcid:vermin4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:vermin4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:vermin4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:vermin4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:vermin4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:vermin4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin4}},   1, True ) /* Stuck */
     , ({{wcid:vermin4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:vermin4}},  12, True ) /* ReportCollisions */
     , ({{wcid:vermin4}},  13, False) /* Ethereal */
     , ({{wcid:vermin4}},  14, True ) /* GravityStatus */
     , ({{wcid:vermin4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:vermin4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:vermin4}},   3,    0.5) /* HealthRate */
     , ({{wcid:vermin4}},   4,      5) /* StaminaRate */
     , ({{wcid:vermin4}},   5,      1) /* ManaRate */
     , ({{wcid:vermin4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:vermin4}},  34,      1) /* PowerupTime */
     , ({{wcid:vermin4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:vermin4}},  39,   1.15) /* DefaultScale */
     , ({{wcid:vermin4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:vermin4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:vermin4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:vermin4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:vermin4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:vermin4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:vermin4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:vermin4}},  64,      1) /* ResistSlash */
     , ({{wcid:vermin4}},  65,      1) /* ResistPierce */
     , ({{wcid:vermin4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:vermin4}},  67,      1) /* ResistFire */
     , ({{wcid:vermin4}},  68,      1) /* ResistCold */
     , ({{wcid:vermin4}},  69,      1) /* ResistAcid */
     , ({{wcid:vermin4}},  70,      1) /* ResistElectric */
     , ({{wcid:vermin4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:vermin4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:vermin4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:vermin4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:vermin4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin4}}, 1, 'Vermin (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:vermin4}},   1, 0x0200EB00) /* Setup */
     , ({{wcid:vermin4}},   2, 0x0900EB00) /* MotionTable */
     , ({{wcid:vermin4}},   4, 0x3000EB00) /* CombatTable */
     , ({{wcid:vermin4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:vermin4}}, 0, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 0, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 1, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 1, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 2, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 2, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 3, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 4, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 5, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 6, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 7, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 7, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 8, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 9, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 10, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 11, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 12, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 12, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 13, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 13, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 14, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 14, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 15, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 15, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 16, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 16, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 17, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 19, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 20, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 21, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 21, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 22, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 23, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 24, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 25, 0x0500EB00, 0x0500EB04)
     , ({{wcid:vermin4}}, 25, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 26, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 27, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 28, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 29, 0x0500EB01, 0x0500EB06)
     , ({{wcid:vermin4}}, 30, 0x0500EB01, 0x0500EB06);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:vermin4}}, 1, 40, 0, 0)
     , ({{wcid:vermin4}}, 2, 40, 0, 0)
     , ({{wcid:vermin4}}, 3, 40, 0, 0)
     , ({{wcid:vermin4}}, 4, 40, 0, 0)
     , ({{wcid:vermin4}}, 5, 10, 0, 0)
     , ({{wcid:vermin4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:vermin4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:vermin4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:vermin4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:vermin4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:vermin4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:vermin4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:vermin4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:vermin4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:vermin4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:vermin4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:vermin4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:vermin4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:vermin4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:vermin4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:vermin4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
