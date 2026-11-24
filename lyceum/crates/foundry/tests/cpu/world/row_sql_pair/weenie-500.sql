INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (500, 'itemtest', 1, '2021-11-01 00:00:00') /* Generic */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (500,  25,         -7) /* Level */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (500,   1, 'Thing') /* Name */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES (500,   1,  99, 0, 0) /* Strength */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES (500,  6, 0, 2, 0,  10, 0, 0) /* MeleeDefense        Trained */;

INSERT INTO `weenie_properties_book` (`object_Id`, `max_Num_Pages`, `max_Num_Chars_Per_Page`)
VALUES (500, 1, 1);
