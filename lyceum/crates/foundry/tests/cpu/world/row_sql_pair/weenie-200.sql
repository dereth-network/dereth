INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (200, 'booktest', 8, '2021-11-01 00:00:00') /* Book */;

INSERT INTO `weenie_properties_book` (`object_Id`, `max_Num_Pages`, `max_Num_Chars_Per_Page`)
VALUES (200, 2, 1000);

INSERT INTO `weenie_properties_book_page_data` (`object_Id`, `page_Id`, `author_Id`, `author_Name`, `author_Account`, `ignore_Author`, `page_Text`)
VALUES (200, 0, 0x00000000, '', 'prewritten', True, 'page one')
     , (200, 1, 0xFFFFFFFF, 'Author', 'prewritten', False, 'page two');
