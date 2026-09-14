Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PrepareRepairEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, WorkClothes, HardHat)
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0250_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_3000_Item_00()
	Controller().PlaceRobot()
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	Controller().StartLocalScene(RobotRepairedScene)
	Controller().MoveActorToMarker(Alias_Actor_Robot, Alias_Marker_RobotPostQuest)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().StopLocalScene(RobotRepairedScene)
	Controller().ScheduleLocalStop()
EndFunction
