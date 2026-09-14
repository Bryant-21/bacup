ObjectReference Function PlayerReference()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Return playerRef
EndFunction

Function SetPlayerValue(ActorValue valueToSet, Float value)
	Actor player = PlayerReference() as Actor
	If player != None && valueToSet != None
		player.SetValue(valueToSet, value)
	EndIf
EndFunction

Function StartBoundScene(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function UnlockAliasDoor(ReferenceAlias doorAlias)
	If doorAlias != None
		ObjectReference doorRef = doorAlias.GetReference()
		If doorRef != None
			doorRef.Lock(False)
			doorRef.BlockActivation(False)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
	If !IsStageDone(550)
		SetStage(550)
	EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
	If !IsStageDone(580)
		SetStage(580)
	EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
	If !IsStageDone(1000)
		SetStage(1000)
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	If !IsStageDone(1200)
		SetStage(1200)
	EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_0007_Item_00()
	If !IsStageDone(570)
		SetStage(570)
	EndIf
EndFunction

Function Fragment_Stage_0008_Item_00()
	If !IsStageDone(800)
		SetStage(800)
	EndIf
EndFunction

Function Fragment_Stage_0009_Item_00()
	If !IsStageDone(580)
		SetStage(580)
	EndIf
EndFunction

Function Fragment_Stage_0012_Item_00()
	If !IsStageDone(1200)
		SetStage(1200)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	ObjectReference playerRef = PlayerReference()
	SetPlayerValue(BURN_SQ01_SpawnAva, 1.0)
	If BURN_SQ01_Radio != None && !BURN_SQ01_Radio.IsRunning() && !BURN_SQ01_Radio.IsCompleted()
		BURN_SQ01_Radio_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0200_Item_01()
	SetObjectiveCompleted(10)
	If !IsStageDone(201)
		SetStage(201)
	EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(21)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveCompleted(21)
	SetObjectiveDisplayed(40)
	SetPlayerValue(BURN_SQ01_SpawnRKIGuards, 1.0)
	If !IsStageDone(iCanyonRaiderSpawnStage)
		SetStage(iCanyonRaiderSpawnStage)
	EndIf
EndFunction

Function Fragment_Stage_0402_Item_00()
	If !IsStageDone(405)
		SetStage(405)
	EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(41)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(41)
	SetObjectiveDisplayed(50)
	SetPlayerValue(BURN_SQ01_SpawnAva, 1.0)
	StartBoundScene(BURN_SQ01_Ava_Reveal)
EndFunction

Function Fragment_Stage_0535_Item_00()
	SetObjectiveCompleted(50)
EndFunction

Function Fragment_Stage_0540_Item_00()
	ObjectReference knockoutFurniture = Alias_Furn_PlayerKnockout_CheckpointInterior.GetReference()
	ObjectReference playerRef = PlayerReference()
	If knockoutFurniture != None && playerRef != None
		knockoutFurniture.Activate(playerRef)
	EndIf
	If !IsStageDone(550)
		SetStage(550)
	EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
	ObjectReference playerRef = PlayerReference()
	If BURNFadeToBlack != None && playerRef != None
		BURNFadeToBlack.Cast(playerRef, playerRef)
	EndIf
	If !IsStageDone(570)
		SetStage(570)
	EndIf
EndFunction

Function Fragment_Stage_0570_Item_00()
	ObjectReference playerRef = PlayerReference()
	ObjectReference destination = Alias_CheckpointTP.GetReference()
	If playerRef != None && destination != None
		playerRef.MoveTo(destination)
	EndIf
	SetPlayerValue(BURN_SQ01_SpawnSilas, 1.0)
	SetPlayerValue(BURN_SQ01_SpawnEugene, 1.0)
	SetPlayerValue(BURN_SQ01_SpawnTheRustKing, 1.0)
	SetPlayerValue(BURN_SQ01_SpawnRKIGuards, 1.0)
	SetObjectiveDisplayed(60)
	If !IsStageDone(580)
		SetStage(580)
	EndIf
EndFunction

Function Fragment_Stage_0580_Item_00()
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0601_Item_00()
	SetPlayerValue(BURN_SQ01_TalkedToEugene_Check, 1.0)
	If IsStageDone(602) && !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0602_Item_00()
	SetPlayerValue(BURN_SQ01_TalkedToSilas_Check, 1.0)
	If IsStageDone(601) && !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	StartBoundScene(BURN_RKI_GuardsWalkToJail)
EndFunction

Function Fragment_Stage_0701_Item_00()
	UnlockAliasDoor(Alias_RKI_Silas_JailDoor)
	UnlockAliasDoor(Alias_RKI_Eugene_Jaildoor)
	UnlockAliasDoor(Alias_RKI_Player_Jaildoor)
	StartBoundScene(BURN_RKI_GuardsUnlockJail)
	StartBoundScene(BURN_EugeneSilasWalkToArena)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0902_Item_00()
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && ChairTeleport != None
		playerRef.MoveTo(ChairTeleport)
	EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
	SetPlayerValue(BURN_SQ01_SpawnSilasKiller, 1.0)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
	SetPlayerValue(BURN_SQ01_ExecuteSilasStageDone, 1.0)
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && BowieKnife != None && playerRef.GetItemCount(BowieKnife) == 0
		playerRef.AddItem(BowieKnife, 1, True)
	EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(110)
	SetPlayerValue(BURN_SQ01_ProveYourResolveStageDone, 1.0)
	StartBoundScene(BURN_RustKing_RaiderArenaWaveStart)
	If !IsStageDone(1110)
		SetStage(1110)
	EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
	SetObjectiveCompleted(110)
	SetObjectiveDisplayed(111)
	StartBoundScene(BURN_RustKing_DeathClawWaveStart)
	If !IsStageDone(1120)
		SetStage(1120)
	EndIf
EndFunction

Function Fragment_Stage_1120_Item_00()
	SetObjectiveCompleted(111)
	SetObjectiveDisplayed(112)
	Actor silas = Alias_Parker.GetActorReference()
	If silas != None && !silas.IsDead()
		silas.Kill()
	EndIf
	StartBoundScene(BURN_RustKing_DeathClawWaveEnd)
	If !IsStageDone(1200)
		SetStage(1200)
	EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveCompleted(110)
	SetObjectiveCompleted(111)
	SetObjectiveCompleted(112)
	SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1280_Item_00()
	ObjectReference knockoutFurniture = Alias_Furn_PlayerKnockout_RustKingdom.GetReference()
	If knockoutFurniture != None
		knockoutFurniture.Enable()
	EndIf
EndFunction

Function Fragment_Stage_1290_Item_00()
	ObjectReference knockoutFurniture = Alias_Furn_PlayerKnockout_RustKingdom.GetReference()
	ObjectReference playerRef = PlayerReference()
	If knockoutFurniture != None && playerRef != None
		knockoutFurniture.Activate(playerRef)
	EndIf
	If !IsStageDone(1300)
		SetStage(1300)
	EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
	ObjectReference playerRef = PlayerReference()
	If BURNFadeToBlack != None && playerRef != None
		BURNFadeToBlack.Cast(playerRef, playerRef)
	EndIf
	If !IsStageDone(1400)
		SetStage(1400)
	EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && RustKingdomExtTeleport != None
		playerRef.MoveTo(RustKingdomExtTeleport)
	EndIf
	If !IsStageDone(1500)
		SetStage(1500)
	EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(120)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	ObjectReference playerRef = PlayerReference()
	If BURN_MQ03_MidQuest_QuestStartKeyword != None && playerRef != None
		BURN_MQ03_MidQuest_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
	FailAllObjectives()
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction
