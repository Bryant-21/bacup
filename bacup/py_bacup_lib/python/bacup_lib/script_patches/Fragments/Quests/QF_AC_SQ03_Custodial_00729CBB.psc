Actor Function ActorFromAlias(ReferenceAlias actorAlias)
	If actorAlias == None
		Return None
	EndIf
	Return actorAlias.GetActorReference()
EndFunction

Actor Function PlayerActor()
	Return ActorFromAlias(Alias_Player)
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
	If targetAlias == None
		Return
	EndIf
	ObjectReference target = targetAlias.GetReference()
	If target != None
		target.Enable()
		Actor targetActor = target as Actor
		If targetActor != None
			targetActor.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function DisableAlias(ReferenceAlias targetAlias)
	If targetAlias != None && targetAlias.GetReference() != None
		targetAlias.GetReference().Disable()
	EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Enable()
		EndIf
		index += 1
	EndWhile
EndFunction

Function DisableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Disable()
		EndIf
		index += 1
	EndWhile
EndFunction

Function EvaluateActor(ReferenceAlias actorAlias)
	Actor target = ActorFromAlias(actorAlias)
	If target != None
		target.Enable()
		target.EvaluatePackage()
	EndIf
EndFunction

Function MakeHostile(ReferenceAlias actorAlias)
	Actor target = ActorFromAlias(actorAlias)
	Actor player = PlayerActor()
	If target == None
		Return
	EndIf
	If AC_SQ03_Custodial_NeutralFaction != None
		target.RemoveFromFaction(AC_SQ03_Custodial_NeutralFaction)
	EndIf
	target.SetGhost(False)
	target.EvaluatePackage()
	If player != None
		target.StartCombat(player)
	EndIf
EndFunction

Function TryFinishPropCollection()
	If IsStageDone(550) && IsStageDone(554) && IsStageDone(558) && !IsStageDone(580)
		SetStage(580)
	EndIf
EndFunction

Function TryFinishCleaning()
	If IsStageDone(510) && IsStageDone(520) && IsStageDone(590) && !IsStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function RemoveSlowSpell()
	Actor player = PlayerActor()
	If player != None && AC_SQ03_Custodial_SlowSpell != None
		player.RemoveSpell(AC_SQ03_Custodial_SlowSpell)
	EndIf
EndFunction

Function RemoveCaptiveChoice()
	Actor player = PlayerActor()
	If player != None && AC_SQ03_Custodial_CaptiveActivatePerk != None
		player.RemovePerk(AC_SQ03_Custodial_CaptiveActivatePerk)
	EndIf
EndFunction

Function FreeVictim()
	Actor victim = ActorFromAlias(Alias_Actor_Victim)
	If victim != None
		If AC_SQ03_Custodial_BoundCaptiveKeyword != None
			victim.RemoveKeyword(AC_SQ03_Custodial_BoundCaptiveKeyword)
		EndIf
		If CaptiveFaction != None
			victim.RemoveFromFaction(CaptiveFaction)
		EndIf
		victim.StopCombat()
		victim.EvaluatePackage()
	EndIf
	RemoveCaptiveChoice()
EndFunction

Function Fragment_Stage_0010_Item_00()
	If !IsStageDone(200)
		EnableAlias(Alias_Actor_Sam_Boardwalk)
	EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
	EnableAlias(Alias_EnableMarker_Pier)
	EnableAlias(Alias_Actor_Sam_Pier)
EndFunction

Function Fragment_Stage_0030_Item_00()
	EnableAlias(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(5)
	EnableAlias(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_0110_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveDisplayed(10)
	EnableAlias(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_0140_Item_00()
	EnableAlias(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
	StartSceneIfStopped(AC_SQ03_Custodial_SamToPier)
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	EvaluateActor(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	EnableAlias(Alias_EnableMarker_Pier)
	EnableAlias(Alias_EnableMarker_Pier_FriendlyNPCs)
	EnableAlias(Alias_Actor_Sam_Pier)
	StartSceneIfStopped(AC_SQ03_Custodial_SamToStage)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
	EvaluateActor(Alias_Actor_Sam_Pier)
EndFunction

Function Fragment_Stage_0499_Item_00()
	If !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
	SetObjectiveDisplayed(55)
	SetObjectiveDisplayed(56)
	SetObjectiveDisplayed(85)
	EnableAlias(Alias_EnableMarker_StageProps)
	EnableCollection(Alias_Activators_TrashPiles)
	EnableCollection(Alias_Activators_Blood)
	EnableCollection(Alias_Activators_AllStageProps)
	EnableCollection(Alias_Notes_LoreItems)
EndFunction

Function Fragment_Stage_0505_Item_00()
	EnableAlias(Alias_EnableMarker_StageProps)
	EnableCollection(Alias_Activators_TrashPiles)
	EnableCollection(Alias_Activators_Blood)
	EnableCollection(Alias_Activators_AllStageProps)
	EnableCollection(Alias_Notes_LoreItems)
EndFunction

Function Fragment_Stage_0510_Item_00()
	SetObjectiveCompleted(50)
	TryFinishCleaning()
EndFunction

Function Fragment_Stage_0520_Item_00()
	SetObjectiveCompleted(55)
	TryFinishCleaning()
EndFunction

Function Fragment_Stage_0540_Item_00()
	SetObjectiveCompleted(85)
EndFunction

Function Fragment_Stage_0550_Item_00()
	DisableAlias(Alias_Activator_StrattonsBriefcase)
	If AC_SQ03_Custodial_OverrideName_Stratton != None
		AC_SQ03_Custodial_OverrideName_Stratton.Show()
	EndIf
	TryFinishPropCollection()
EndFunction

Function Fragment_Stage_0554_Item_00()
	DisableAlias(Alias_Activator_PropGun)
	TryFinishPropCollection()
EndFunction

Function Fragment_Stage_0558_Item_00()
	DisableAlias(Alias_Activator_FeatherBoa)
	TryFinishPropCollection()
EndFunction

Function Fragment_Stage_0580_Item_00()
	SetObjectiveCompleted(56)
	SetObjectiveDisplayed(57)
EndFunction

Function Fragment_Stage_0590_Item_00()
	SetObjectiveCompleted(57)
	TryFinishCleaning()
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveCompleted(55)
	SetObjectiveCompleted(56)
	SetObjectiveCompleted(57)
	SetObjectiveDisplayed(60)
	EvaluateActor(Alias_Actor_Sam_Pier)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	EnableAlias(Alias_Activator_SuspiciousTrunk)
EndFunction

Function Fragment_Stage_0750_Item_00()
	Actor player = PlayerActor()
	If player != None && AC_SQ03_Custodial_SlowSpell != None && !player.HasSpell(AC_SQ03_Custodial_SlowSpell)
		player.AddSpell(AC_SQ03_Custodial_SlowSpell, False)
	EndIf
	If AC_SQ03_Custodial_TrunkMessage != None
		AC_SQ03_Custodial_TrunkMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
	RemoveSlowSpell()
	DisableAlias(Alias_Activator_SuspiciousTrunk)
	If AC_SQ03_Custodial_TrunkDisposedMessage != None
		AC_SQ03_Custodial_TrunkDisposedMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
	EvaluateActor(Alias_Actor_Sam_Pier)
EndFunction

Function Fragment_Stage_0959_Item_00()
	If !IsStageDone(960)
		SetStage(960)
	EndIf
EndFunction

Function Fragment_Stage_0960_Item_00()
	StartSceneIfStopped(AC_SQ03_Custodial_SamToAlley)
	EnableAlias(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
	EvaluateActor(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1010_Item_00()
	If IsStageDone(1000) && !IsStageDone(1020)
		SetStage(1020)
	EndIf
EndFunction

Function Fragment_Stage_1020_Item_00()
	StartSceneIfStopped(AC_SQ03_Custodial_SamInAlley)
EndFunction

Function Fragment_Stage_1050_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
	EvaluateActor(Alias_Actor_Client)
EndFunction

Function Fragment_Stage_1055_Item_00()
	EvaluateActor(Alias_Actor_Client)
EndFunction

Function Fragment_Stage_1060_Item_00()
	StartSceneIfStopped(AC_SQ03_Custodial_ClientMeeting)
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(110)
	SetObjectiveDisplayed(120)
	EvaluateActor(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveDisplayed(125)
	EvaluateActor(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1250_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveDisplayed(125)
	EvaluateActor(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1260_Item_00()
	If IsStageDone(1250) && !IsStageDone(1270)
		SetStage(1270)
	EndIf
EndFunction

Function Fragment_Stage_1270_Item_00()
	StartSceneIfStopped(AC_SQ03_Custodial_SamSendPlayerToBackRoom)
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetObjectiveCompleted(125)
	SetObjectiveDisplayed(130)
	Actor player = PlayerActor()
	If player != None && AC_SQ03_Custodial_BackRoomKey != None && player.GetItemCount(AC_SQ03_Custodial_BackRoomKey) < 1
		player.AddItem(AC_SQ03_Custodial_BackRoomKey, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_1399_Item_00()
	If !IsStageDone(1400)
		SetStage(1400)
	EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
	SetObjectiveCompleted(130)
	SetObjectiveDisplayed(140)
	Actor victim = ActorFromAlias(Alias_Actor_Victim)
	Actor player = PlayerActor()
	If victim != None
		If CaptiveFaction != None
			victim.AddToFaction(CaptiveFaction)
		EndIf
		If AC_SQ03_Custodial_BoundCaptiveKeyword != None
			victim.AddKeyword(AC_SQ03_Custodial_BoundCaptiveKeyword)
		EndIf
		victim.Enable()
		victim.EvaluatePackage()
	EndIf
	If player != None && AC_SQ03_Custodial_CaptiveActivatePerk != None
		player.AddPerk(AC_SQ03_Custodial_CaptiveActivatePerk)
	EndIf
	StartSceneIfStopped(AC_SQ03_Custodial_PlayerEntersBackRoom)
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(140)
	SetObjectiveDisplayed(150)
	SetObjectiveDisplayed(152)
	SetObjectiveDisplayed(154)
	SetObjectiveDisplayed(156)
EndFunction

Function Fragment_Stage_1501_Item_00()
	MakeHostile(Alias_Actor_Sam_Boardwalk)
EndFunction

Function Fragment_Stage_1550_Item_00()
	SetObjectiveCompleted(152)
	SetObjectiveFailed(156)
	Actor player = PlayerActor()
	If player != None && AC_SQ03_Custodial_PlayerKilledVictim != None
		player.SetValue(AC_SQ03_Custodial_PlayerKilledVictim, 1.0)
	EndIf
	RemoveCaptiveChoice()
	StartSceneIfStopped(AC_SQ03_Custodial_SamReactionToDeadVictim)
EndFunction

Function Fragment_Stage_1560_Item_00()
	SetObjectiveCompleted(154)
	SetObjectiveCompleted(156)
	FreeVictim()
	StartSceneIfStopped(AC_SQ03_Custodial_VictimReactionToDeadSam)
	StartSceneIfStopped(AC_SQ03_Custodial_VictimReactionToFreed)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(150)
	RemoveSlowSpell()
	RemoveCaptiveChoice()
	Actor sam = ActorFromAlias(Alias_Actor_Sam_Boardwalk)
	If sam != None && AC_SQ03_Custodial_SaltwaterSam_AwayValue != None
		sam.SetValue(AC_SQ03_Custodial_SaltwaterSam_AwayValue, 1.0)
	EndIf
	StartSceneIfStopped(AC_SQ03_Custodial_SamFinalComment)
EndFunction

Function Fragment_Stage_9100_Item_00()
	Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
	RemoveSlowSpell()
	RemoveCaptiveChoice()
	DisableCollection(Alias_Activators_TrashPiles)
	DisableCollection(Alias_Activators_Blood)
	DisableCollection(Alias_Activators_AllStageProps)
	DisableCollection(Alias_Notes_LoreItems)
	DisableAlias(Alias_EnableMarker_StageProps)
	DisableAlias(Alias_EnableMarker_Pier)
EndFunction
