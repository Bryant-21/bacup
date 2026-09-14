Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Actor sentryBot = GetLinkedRef(Vault79_SentryBot_Keyword) as Actor
	ObjectReference podDoor = GetLinkedRef(Vault79_SentryPodDoor_Keyword)
	Bool encounterComplete = sentryBot != None && sentryBot.IsDead()

	If myActorValue != None
		encounterComplete = encounterComplete \
			|| (playerRef != None && playerRef.GetValue(myActorValue) >= 1.0) \
			|| (sentryBot != None && sentryBot.GetValue(myActorValue) >= 1.0)
	EndIf

	If encounterComplete
		If podDoor != None
			podDoor.SetOpen(True)
		EndIf
		Return
	EndIf

	If IsActivationBlocked()
		Return
	EndIf

	BlockActivation(True, True)

	ObjectReference soundMarker = GetLinkedRef(Vault79_SentryPodSoundMarker_Keyword)
	If SentryPodSoundStart != None && soundMarker != None
		SentryPodSoundStart.Play(soundMarker)
	EndIf

	ObjectReference klaxonMarker = GetLinkedRef(Vault79_KlaxonMarker_Keyword)
	If klaxonMarker != None
		klaxonMarker.Activate(Self)
	EndIf

	ObjectReference steamMarker = GetLinkedRef(Vault79_SteamEnableMarker_Keyword)
	If steamMarker != None
		steamMarker.Enable()
	EndIf

	If podDoor != None
		podDoor.SetOpen(True)
	EndIf

	ObjectReference sentryActivator = GetLinkedRef(Vault79_SentryActivator_Keyword)
	If sentryActivator != None
		sentryActivator.Activate(Self)
	EndIf

	If sentryBot != None && sentryBot.IsDisabled()
		sentryBot.Enable()
	EndIf
EndEvent
