Event OnActivate(ObjectReference akActionRef)
	If IsActivationBlocked()
		Return
	EndIf

	BlockActivation(True, True)

	sBangingOnDoor.Play(myDoorDummy)
	sGhoulNoise.Play(myGhoulSoundMarker)
	If myGhoulSound2Marker != None
		sGhoulNoise.Play(myGhoulSound2Marker)
	EndIf

	myKlaxonDummy.Activate(Self)
	ObjectReference[] klaxonSounds = myKlaxonDummy.GetLinkedRefChain(LinkCustom01)
	Int soundIndex = 0
	While soundIndex < klaxonSounds.Length
		klaxonSounds[soundIndex].EnableNoWait()
		soundIndex += 1
	EndWhile
	myDoorDummy.Activate(Self)
	myActorEnableMarker.Enable()

	Actor playerRef = Game.GetPlayer()
	If playerRef.GetValue(myActorValue) < 1.0
		myBossEnableLegendaryMarker.Enable()
	Else
		myBossEnableMarker.Enable()
	EndIf
EndEvent
