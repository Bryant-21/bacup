Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PrepareRepairEncounter()
	SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, Armor_LabCoat, Armor_EyeGlasses)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_0210_Item_00()
	Controller().StartLocalScene(Scene_Ambient)
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveDisplayed(10)
	Controller().StartLocalScene(Scene_Robot)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_3000_Item_00()
	SetObjectiveDisplayed(20)
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_7000_Item_00()
	SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	Actor robot = Alias_Actor_Robot.GetActorReference()
	If robot != None && RobotMod != None
		robot.AttachMod(RobotMod)
		robot.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_8100_Item_00()
	Actor robot = Alias_Actor_Robot.GetActorReference()
	If robot != None
		If Alias_Markers_RobotLocs_PostComplete.GetCount() > 0
			robot.MoveTo(Alias_Markers_RobotLocs_PostComplete.GetAt(0))
		EndIf
		robot.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(Scene_Ambient)
	Controller().StopLocalScene(Scene_Robot)
	Controller().ScheduleLocalStop()
EndFunction
