Quests:XPD_HubRE:XPD_HubRE_LostAndFound_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_LostAndFound_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PrepareLostAndFoundEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ChosenLocation)
	Controller().EquipActor(Alias_Actor_Character, Outfit_NPC)
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0210_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0260_Item_00()
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().ScheduleLocalStop()
EndFunction
