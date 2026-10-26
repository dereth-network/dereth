-- The game's species and materials by number, as a monster's species (its integer property 2)
-- and an item's material name them: for the monster list's `species` and VTank's dump commands.

local N = {}

N.SPECIES = {
	[1] = "Olthoi", [2] = "Banderling", [3] = "Drudge", [4] = "Mosswart", [5] = "Lugian",
	[6] = "Tumerok", [7] = "Mite", [8] = "Tusker", [9] = "Phyntos Wasp", [10] = "Rat",
	[11] = "Auroch", [12] = "Cow", [13] = "Golem", [14] = "Undead", [15] = "Gromnie",
	[16] = "Reedshark", [17] = "Armoredillo", [18] = "Fae", [19] = "Virindi", [20] = "Wisp",
	[21] = "Knathtead", [22] = "Shadow", [23] = "Mattekar", [24] = "Mumiyah", [25] = "Rabbit",
	[26] = "Sclavus", [27] = "Shallows Shark", [28] = "Monouga", [29] = "Zefir", [30] = "Skeleton",
	[31] = "Human", [32] = "Shreth", [33] = "Chittick", [34] = "Moarsman", [35] = "Olthoi Larvae",
	[36] = "Slithis", [37] = "Deru", [38] = "Fire Elemental", [39] = "Snowman", [40] = "Unknown",
	[41] = "Bunny", [42] = "Lightning Elemental", [43] = "Rockslide", [44] = "Grievver",
	[45] = "Niffis", [46] = "Ursuin", [47] = "Crystal", [48] = "Hollow Minion", [49] = "Scarecrow",
	[50] = "Idol", [51] = "Empyrean", [52] = "Hopeslayer", [53] = "Doll", [54] = "Marionette",
	[55] = "Carenzi", [56] = "Siraluun", [57] = "Aun Tumerok", [58] = "Hea Tumerok",
	[59] = "Simulacrum", [60] = "Acid Elemental", [61] = "Frost Elemental", [62] = "Elemental",
	[63] = "Statue", [64] = "Wall", [65] = "Altered Human", [66] = "Device", [67] = "Harbinger",
	[68] = "Dark Sarcophagus", [69] = "Chicken", [70] = "Gotrok Lugian", [71] = "Margul",
	[72] = "Bleached Rabbit", [73] = "Nasty Rabbit", [74] = "Grimacing Rabbit", [75] = "Burun",
	[76] = "Target", [77] = "Ghost", [78] = "Fiun", [79] = "Eater", [80] = "Penguin", [81] = "Ruschk",
	[82] = "Thrungus", [83] = "Viamontian Knight", [84] = "Remoran", [85] = "Swarm", [86] = "Moar",
	[87] = "Enchanted Arms", [88] = "Sleech", [89] = "Mukkir", [90] = "Merwart", [91] = "Food",
	[92] = "Paradox Olthoi", [93] = "Harvest", [94] = "Energy", [95] = "Apparition", [96] = "Aerbax",
	[97] = "Touched", [98] = "Blighted Moarsman", [99] = "Gear Knight", [100] = "Gurog",
	[101] = "Anekshay",
}

N.MATERIALS = {
	[1] = "Ceramic", [2] = "Porcelain", [3] = "Cloth", [4] = "Linen", [5] = "Satin", [6] = "Silk",
	[7] = "Velvet", [8] = "Wool", [9] = "Gem", [10] = "Agate", [11] = "Amber", [12] = "Amethyst",
	[13] = "Aquamarine", [14] = "Azurite", [15] = "Black Garnet", [16] = "Black Opal",
	[17] = "Bloodstone", [18] = "Carnelian", [19] = "Citrine", [20] = "Diamond", [21] = "Emerald",
	[22] = "Fire Opal", [23] = "Green Garnet", [24] = "Green Jade", [25] = "Hematite",
	[26] = "Imperial Topaz", [27] = "Jet", [28] = "Lapis Lazuli", [29] = "Lavender Jade",
	[30] = "Malachite", [31] = "Moonstone", [32] = "Onyx", [33] = "Opal", [34] = "Peridot",
	[35] = "Red Garnet", [36] = "Red Jade", [37] = "Rose Quartz", [38] = "Ruby", [39] = "Sapphire",
	[40] = "Smokey Quartz", [41] = "Sunstone", [42] = "Tiger Eye", [43] = "Tourmaline",
	[44] = "Turquoise", [45] = "White Jade", [46] = "White Quartz", [47] = "White Sapphire",
	[48] = "Yellow Garnet", [49] = "Yellow Topaz", [50] = "Zircon", [51] = "Ivory", [52] = "Leather",
	[53] = "Armoredillo Hide", [54] = "Gromnie Hide", [55] = "Reed Shark Hide", [56] = "Metal",
	[57] = "Brass", [58] = "Bronze", [59] = "Copper", [60] = "Gold", [61] = "Iron", [62] = "Pyreal",
	[63] = "Silver", [64] = "Steel", [65] = "Stone", [66] = "Alabaster", [67] = "Granite",
	[68] = "Marble", [69] = "Obsidian", [70] = "Sandstone", [71] = "Serpentine", [72] = "Wood",
	[73] = "Ebony", [74] = "Mahogany", [75] = "Oak", [76] = "Pine", [77] = "Teak",
}

return N
