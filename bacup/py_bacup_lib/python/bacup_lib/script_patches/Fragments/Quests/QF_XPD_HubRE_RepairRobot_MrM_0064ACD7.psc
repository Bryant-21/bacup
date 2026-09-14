Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PrepareRepairEncounter()
	SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_0210_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0220_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0250_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveDisplayed(20)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	Actor robot = Alias_Actor_Robot.GetActorReference()
	If robot != None && RobotHat != None
		robot.AttachMod(RobotHat)
		robot.EvaluatePackage()
	EndIf
	Controller().StartLocalScene(Scene_RobotFixed)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().StopLocalScene(Scene_RobotFixed)
	Controller().ScheduleLocalStop()
EndFunction
