Quests:XPD_HubRE:XPD_HubRE_TakePhoto_QuestScript Function Controller()
	Return (Self as Quest) as Quests:XPD_HubRE:XPD_HubRE_TakePhoto_QuestScript
EndFunction

Function GiveCamera()
	Actor player = Alias_Player.GetActorReference()
	If player != None && Bucketlist_Camera != None && player.GetItemCount(Bucketlist_Camera) < 1
		player.AddItem(Bucketlist_Camera, 1, True)
	EndIf
	Controller().BeginLocalPhotoValidation(Bucketlist_Camera)
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PreparePhotoEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, CryptidOutfit, TinFoilHat)
EndFunction

Function Fragment_Stage_0250_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_3000_Item_00()
	GiveCamera()
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	Controller().StartLocalScene(Scene_Reaction_Suspect)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().StopLocalScene(Scene_Reaction_Suspect)
	Controller().ScheduleLocalStop()
EndFunction
