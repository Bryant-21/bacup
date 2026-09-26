Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || akSenderRef == None
		Return
	EndIf
	Quest owner = GetOwningQuest()
	E09C_QuestScript eventScript = owner as E09C_QuestScript
	If owner == None || eventScript == None || !owner.IsRunning()
		Return
	EndIf
	If !owner.IsStageDone(eventScript.iTrackStartStage) || owner.IsStageDone(eventScript.iTrackStageToSetOnComplete)
		Return
	EndIf
	; The repair activator links to its track piece and is enabled as the piece's opposite, so restoring
	; the piece also retires the activator.
	ObjectReference piece = akSenderRef.GetLinkedRef()
	If piece == None || !piece.IsDisabled()
		Return
	EndIf
	piece.Enable(False)
	eventScript.TrackRepaired()
EndEvent
