Quests:XPD_HubRE:XPD_HubRE_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().SelectSceneLocation()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0101_Item_00()
EndFunction

Function Fragment_Stage_0102_Item_00()
EndFunction

Function Fragment_Stage_0103_Item_00()
EndFunction

Function Fragment_Stage_0150_Item_00()
	Controller().MoveActorToSceneCenter(Alias_Actor_1)
	Controller().MoveActorToSceneCenter(Alias_Actor_2)
	If IsStageDone(101)
		Controller().EquipActor(Alias_Actor_1, Outfit_A_Radiation)
		Controller().EquipActor(Alias_Actor_2, Outfit_B_Radiation)
	ElseIf IsStageDone(102)
		Controller().EquipActor(Alias_Actor_1, Outfit_A_PA)
		Controller().EquipActor(Alias_Actor_2, Outfit_B_PA)
	ElseIf IsStageDone(103)
		Controller().EquipActor(Alias_Actor_1, Outfit_A_Dirt)
		Controller().EquipActor(Alias_Actor_2, Outfit_B_Dirt)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_2100_Item_00()
	SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_2200_Item_00()
	SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_2300_Item_00()
	SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(10)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().ScheduleLocalStop()
EndFunction
