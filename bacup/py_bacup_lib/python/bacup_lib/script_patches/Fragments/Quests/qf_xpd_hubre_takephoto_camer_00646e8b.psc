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

Int Function ChosenPhotoStage()
	Int stage = 1100
	While stage <= 1700
		If IsStageDone(stage)
			Return stage
		EndIf
		stage += 100
	EndWhile
	Return -1
EndFunction

Function DisplayChosenTarget()
	Int targetStage = ChosenPhotoStage()
	If targetStage >= 1100 && targetStage <= 1700
		SetObjectiveDisplayed(20 + ((targetStage - 1100) / 100))
	EndIf
EndFunction

Function StartTargetReaction()
	Int targetStage = ChosenPhotoStage()
	If targetStage == 1100
		Controller().StartLocalScene(Scene_Reaction_Esme)
	ElseIf targetStage == 1200
		Controller().StartLocalScene(Scene_Reaction_Rucker)
	ElseIf targetStage == 1300
		Controller().StartLocalScene(Scene_Reaction_Ellison)
	ElseIf targetStage == 1400
		Controller().StartLocalScene(Scene_Reaction_Skippy)
	ElseIf targetStage == 1500
		Controller().StartLocalScene(Scene_Reaction_Sophie)
	ElseIf targetStage == 1700
		Controller().StartLocalScene(Scene_Reaction_Orlando)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	Controller().PreparePhotoEncounter()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	Controller().MoveActorToMarker(Alias_Actor_Character, Alias_Marker_ActorSpawn)
	Controller().EquipActor(Alias_Actor_Character, FanClothes)
EndFunction

Function Fragment_Stage_0250_Item_00()
	Controller().StartLocalScene(AmbientScene)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10)
	DisplayChosenTarget()
EndFunction

Function Fragment_Stage_3000_Item_00()
	GiveCamera()
EndFunction

Function Fragment_Stage_8000_Item_00()
	Int objective = 20
	While objective <= 26
		SetObjectiveCompleted(objective)
		objective += 1
	EndWhile
	SetObjectiveDisplayed(30)
	StartTargetReaction()
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(30)
	Controller().RecordEncounter(Alias_Player, NumEncounters)
	Controller().StopLocalScene(AmbientScene)
	Controller().ScheduleLocalStop()
EndFunction
