Function RestoreMasterHolotape()
	Actor playerRef = FS03Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None || GetCurrentStageID() < iLoadHoloArmory || GetStageDone(1000)
		Return
	EndIf

	ObjectReference holotapeRef = MasterHolotape.GetReference()
	If holotapeRef == None || holotapeRef.GetContainer() == playerRef
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
	If FS02_MQ_Reassembly != None && !FS02_MQ_Reassembly.GetStageDone(1000)
		FS02_MQ_Reassembly.SetStage(1000)
	EndIf
	RestoreMasterHolotape()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == iLoadHoloArmory || auiStageID == iLoadHoloRaleigh || auiStageID == iSamUnlocked || auiStageID == iLoadHoloRelay
		RestoreMasterHolotape()
	EndIf
EndEvent
