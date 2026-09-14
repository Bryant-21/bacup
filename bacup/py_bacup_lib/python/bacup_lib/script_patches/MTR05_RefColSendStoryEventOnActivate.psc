Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference activatorRef = GetAt(index)
		If activatorRef != None
			RegisterForRemoteEvent(activatorRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || MTR05_Main_StartKeyword == None
		Return
	EndIf
	If MTR05_Main_ActiveKeyword == None || !playerRef.HasKeyword(MTR05_Main_ActiveKeyword)
		MTR05_Main_StartKeyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Motherlode.GetReference(), iStageToSetOnStart)
	EndIf
EndEvent
