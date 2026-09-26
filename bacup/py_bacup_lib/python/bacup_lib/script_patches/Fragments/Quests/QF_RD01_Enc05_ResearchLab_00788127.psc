; Alias_EnableMarker_Wave01 converted to alias -1 (the FO76 alias no longer exists and its ref
; type has no refs), so the fragments never touch it; the quest script spawns the waves.
Function Fragment_Stage_0100_Item_00()
	Raids:RD01:Enc05:QuestScript encounter = (Self as Quest) as Raids:RD01:Enc05:QuestScript
	If encounter != None
		encounter.StartEncounter()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	Raids:RD01:Enc05:QuestScript encounter = (Self as Quest) as Raids:RD01:Enc05:QuestScript
	If encounter != None
		encounter.EncounterVictory()
	EndIf
EndFunction
