Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = FS02Player.GetActorReference()
	ObjectReference detectorRef = GetReference()
	If owningQuest == None || playerRef == None || detectorRef == None || akActionRef != playerRef
		Return
	EndIf
	If owningQuest.GetStageDone(StageToSet) || playerRef.GetItemCount(FS01_MQ_Warn_UpgradedTransmitter) < 1
		Return
	EndIf

	If DetectorAlias.GetReference() != detectorRef
		DetectorAlias.ForceRefTo(detectorRef)
	EndIf
	playerRef.RemoveItem(FS01_MQ_Warn_UpgradedTransmitter, 1, True)
	If !owningQuest.GetStageDone(StageToSet)
		owningQuest.SetStage(StageToSet)
	EndIf
EndEvent
