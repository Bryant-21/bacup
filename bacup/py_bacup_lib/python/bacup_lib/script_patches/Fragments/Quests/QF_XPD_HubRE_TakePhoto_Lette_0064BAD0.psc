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

Function MoveToPhotoLocation()
	Controller().MoveActorToSceneCenter(Alias_Actor_Character)
	SetObjectiveCompleted(18)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PreparePhotoEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, Clothes_Casual)
EndFunction

Function Fragment_Stage_0225_Item_00()
	Controller().StartLocalScene(Scene_Ambient)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(12)
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveDisplayed(12)
EndFunction

Function Fragment_Stage_2000_Item_00()
	GiveCamera()
EndFunction

Function Fragment_Stage_3000_Item_00()
	SetObjectiveCompleted(12)
	SetObjectiveDisplayed(14)
EndFunction

Function Fragment_Stage_3100_Item_00()
	Controller().EquipActor(Alias_Actor_Character, Clothes_Casual)
EndFunction

Function Fragment_Stage_3200_Item_00()
	Controller().EquipActor(Alias_Actor_Character, Clothes_Formal)
EndFunction

Function Fragment_Stage_3300_Item_00()
	Controller().EquipActor(Alias_Actor_Character, Clothes_Silly)
EndFunction

Function Fragment_Stage_4000_Item_00()
	SetObjectiveCompleted(14)
	SetObjectiveDisplayed(16)
EndFunction

Function Fragment_Stage_4100_Item_00()
	Actor subject = Alias_Actor_Character.GetActorReference()
	If subject != None
		subject.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_4200_Item_00()
	Actor subject = Alias_Actor_Character.GetActorReference()
	If subject != None && SeriousIdle != None
		subject.PlayIdle(SeriousIdle)
	EndIf
EndFunction

Function Fragment_Stage_4300_Item_00()
	Actor subject = Alias_Actor_Character.GetActorReference()
	If subject != None
		subject.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_5000_Item_00()
	SetObjectiveCompleted(16)
	SetObjectiveDisplayed(18)
EndFunction

Function Fragment_Stage_5100_Item_00()
	MoveToPhotoLocation()
EndFunction

Function Fragment_Stage_5200_Item_00()
	MoveToPhotoLocation()
EndFunction

Function Fragment_Stage_5300_Item_00()
	MoveToPhotoLocation()
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(18)
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(Scene_Ambient)
	Controller().ScheduleLocalStop()
EndFunction
