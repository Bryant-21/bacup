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

Function SetupPartyActor(ReferenceAlias partyActor, Armor outfit, Armor headwear)
	Controller().MoveActorToSceneCenter(partyActor)
	Controller().EquipActor(partyActor, outfit, headwear)
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PreparePhotoEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	SetupPartyActor(Alias_Actor_PartyGoer, Outfit_ClownOutfit, Headwear_ClownOutfit)
	SetupPartyActor(Alias_Actor_PartyGoer02, Outfit_ClownOutfit, PartyHat)
	SetupPartyActor(Alias_Actor_PartyGoer03, Outfit_ClownOutfit, PartyHat)
EndFunction

Function Fragment_Stage_0220_Item_00()
	Controller().StartLocalScene(Ambient_Attract)
EndFunction

Function Fragment_Stage_2000_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(15)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_2100_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_2200_Item_00()
	GiveCamera()
EndFunction

Function Fragment_Stage_3000_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	GiveCamera()
EndFunction

Function Fragment_Stage_3100_Item_00()
	Controller().StartLocalScene(Ambient_Party)
EndFunction

Function Fragment_Stage_4200_Item_00()
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_4300_Item_00()
	SetObjectiveCompleted(45)
	Actor partyGoer = Alias_Actor_PartyGoer.GetActorReference()
	If partyGoer != None && Hatchet != None
		partyGoer.AddItem(Hatchet, 1, True)
		partyGoer.EquipItem(Hatchet, False, True)
	EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
	Controller().StartLocalScene(Ambient_PhotoTaken)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(40)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(Ambient_Attract)
	Controller().StopLocalScene(Ambient_Party)
	RegisterForPostCompleteLoad()
EndFunction

Function Fragment_Stage_9100_Item_00()
	BeginPostCompleteScene()
EndFunction

Function RegisterForPostCompleteLoad()
	Actor player = Game.GetPlayer()
	If player != None
		RegisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
EndFunction

Function BeginPostCompleteScene()
	RegisterForPostCompleteLoad()
	If Ambient_PostComplete == None
		CompletePostCompleteEncounter()
		Return
	EndIf

	RegisterForRemoteEvent(Ambient_PostComplete, "OnEnd")
	Controller().StartLocalScene(Ambient_PostComplete)
	If !Ambient_PostComplete.IsPlaying()
		CompletePostCompleteEncounter()
	EndIf
EndFunction

Event Scene.OnEnd(Scene akSender)
	If akSender == Ambient_PostComplete
		CompletePostCompleteEncounter()
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender != Game.GetPlayer() || !IsRunning() || !IsStageDone(9000)
		Return
	EndIf

	If IsStageDone(9200)
		CompletePostCompleteEncounter()
	ElseIf IsStageDone(9100)
		If Ambient_PostComplete != None && Ambient_PostComplete.IsPlaying()
			RegisterForRemoteEvent(Ambient_PostComplete, "OnEnd")
		Else
			CompletePostCompleteEncounter()
		EndIf
	EndIf
EndEvent

Function CompletePostCompleteEncounter()
	If !IsStageDone(9200)
		SetStage(9200)
	EndIf
	If IsStageDone(9200) && IsRunning()
		UnregisterPostCompleteEvents()
		Stop()
	EndIf
EndFunction

Function UnregisterPostCompleteEvents()
	Actor player = Game.GetPlayer()
	If player != None
		UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
	If Ambient_PostComplete != None
		UnregisterForRemoteEvent(Ambient_PostComplete, "OnEnd")
	EndIf
EndFunction

Event OnQuestShutdown()
	UnregisterPostCompleteEvents()
EndEvent
