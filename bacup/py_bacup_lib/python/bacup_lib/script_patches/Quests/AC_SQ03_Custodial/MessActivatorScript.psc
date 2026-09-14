Function InitializeMesses()
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If PlayerRef == None
		PlayerRef = Alias_Player.GetActorReference()
	EndIf
	If MessesCleanedReq <= 0
		MessesCleanedReq = GetCount()
	EndIf
EndFunction

Event OnAliasInit()
	InitializeMesses()
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	InitializeMesses()
	If OwningQuest == None || akSenderRef == None || akActionRef != PlayerRef
		Return
	EndIf
	If Find(akSenderRef) < 0
		Return
	EndIf

	If PlaceRefOnActivate != None
		akSenderRef.PlaceAtMe(PlaceRefOnActivate, 1, False, False, True)
	EndIf
	If AddKeywordOnActivate != None
		akSenderRef.AddKeyword(AddKeywordOnActivate)
	EndIf
	If RemoveFromAliasOnActivate
		RemoveRef(akSenderRef)
	EndIf
	If DisableOnActivate
		akSenderRef.Disable()
	EndIf

	MessesCleaned += 1
	If MessesCleanedReq > 0 && MessesCleaned >= MessesCleanedReq && Stage_AllMessesCleaned > 0 && !OwningQuest.IsStageDone(Stage_AllMessesCleaned)
		OwningQuest.SetStage(Stage_AllMessesCleaned)
	EndIf
EndEvent
