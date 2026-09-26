Function RestoreMasterHolotape()
	Actor playerRef = FS03Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None || GetCurrentStageID() < iLoadHoloArmory || GetStageDone(1000)
		Return
	EndIf

	ObjectReference holotapeRef = MasterHolotape.GetReference()
	If holotapeRef == None
		Return
	EndIf
	RegisterForRemoteEvent(holotapeRef, "OnHolotapePlay")
	If holotapeRef.GetContainer() == playerRef
		Return
	EndIf
	If holotapeRef.GetContainer() != None
		holotapeRef.GetContainer().RemoveItem(holotapeRef, 1, True, playerRef)
	Else
		playerRef.AddItem(holotapeRef, 1, True)
	EndIf
EndFunction

Event OnQuestInit()
	Parent.OnQuestInit()
	RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	If FS02_MQ_Reassembly != None && !FS02_MQ_Reassembly.GetStageDone(1000)
		FS02_MQ_Reassembly.SetStage(1000)
	EndIf
	RestoreMasterHolotape()
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference aTerminalRef)
	If MasterHolotape == None || akSender != MasterHolotape.GetReference() || !IsRunning() || GetStageDone(1000)
		Return
	EndIf
	FS03_MQ_Fruition_MasterHoloScript terminalController = Game.GetFormFromFile(0x0019C272, "SeventySix.esm") as FS03_MQ_Fruition_MasterHoloScript
	If terminalController != None
		terminalController.HandleHolotapePlay(aTerminalRef)
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	RestoreMasterHolotape()
EndEvent

Event OnQuestShutdown()
	UnregisterForAllRemoteEvents()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == iLoadHoloArmory || auiStageID == iLoadHoloRaleigh || auiStageID == iSamUnlocked || auiStageID == iLoadHoloRelay
		RestoreMasterHolotape()
	EndIf
EndEvent
