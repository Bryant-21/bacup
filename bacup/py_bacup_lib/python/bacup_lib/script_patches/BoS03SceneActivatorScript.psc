Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && SceneToPlay != None && !SceneToPlay.IsPlaying()
		SceneToPlay.Start()
	EndIf
EndEvent
