Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_RepairRobot_QuestScript
EndFunction

Function RemoveTape()
	Actor player = Alias_Player.GetActorReference()
	If player != None && RustyHolotape != None
		Int tapeCount = player.GetItemCount(RustyHolotape)
		If tapeCount > 0
			player.RemoveItem(RustyHolotape, tapeCount, True)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PrepareRepairEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, Clothing)
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
	Actor player = Alias_Player.GetActorReference()
	If player != None && RustyHolotape != None && player.GetItemCount(RustyHolotape) < 1
		player.AddItem(RustyHolotape, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_8100_Item_00()
	RemoveTape()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_8200_Item_00()
	Controller().PlaceRobot()
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().ScheduleLocalStop()
EndFunction
