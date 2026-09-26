Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef
		Return
	EndIf
	MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
	If questScript == None
		Return
	EndIf
	If questScript.SetTable(akSenderRef, playerRef) <= 0 && UIMenuCancel != None
		UIMenuCancel.Play(akSenderRef)
	EndIf
EndEvent
