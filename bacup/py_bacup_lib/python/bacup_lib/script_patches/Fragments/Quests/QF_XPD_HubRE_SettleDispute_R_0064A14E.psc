Quests:XPD_HubRE:XPD_HubRE_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().SelectSceneLocation()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
	Controller().MoveActorToSceneCenter(Alias_Actor_1)
	Controller().MoveActorToSceneCenter(Alias_Actor_2)
	Controller().EquipActor(Alias_Actor_1, Outfit_RespondersUniform)
	Controller().EquipActor(Alias_Actor_2, Outfit_RecruitUniform)
EndFunction

Function Fragment_Stage_0299_Item_00()
	Controller().StartLocalScene(Scene_Ambient)
EndFunction

Function Fragment_Stage_2000_Item_00()
	Actor recruit = Alias_Actor_2.GetActorReference()
	Controller().EquipActor(Alias_Actor_2, Outfit_RespondersUniform)
	If recruit != None && Recruited != None
		recruit.SetValue(Recruited, 1.0)
		recruit.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(10)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(Scene_Ambient)
	Controller().ScheduleLocalStop()
EndFunction
