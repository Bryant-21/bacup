State waiting
	Event OnTriggerEnter(ObjectReference akActionRef)
		Actor playerRef = akActionRef as Actor
		If playerRef != Game.GetPlayer() || QuestIDKeyword == None
			Return
		EndIf
		Int index = 0
		While index < MyExcludedActorValueSettings.Length
			If MyExcludedActorValueSettings[index].myValue != None && playerRef.GetValue(MyExcludedActorValueSettings[index].myValue) >= MyExcludedActorValueSettings[index].myValueSetting
				Return
			EndIf
			index += 1
		EndWhile
		If QuestIDKeyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Self)
			index = 0
			While index < MyTrackedActorValueSettings.Length
				If MyTrackedActorValueSettings[index].myValue != None
					playerRef.SetValue(MyTrackedActorValueSettings[index].myValue, MyTrackedActorValueSettings[index].myValueSetting as Float)
				EndIf
				index += 1
			EndWhile
			GoToState("done")
			If ResetTimeSeconds >= 0.0
				StartTimer(ResetTimeSeconds)
			EndIf
		EndIf
	EndEvent
EndState

State done
	Event OnTimer(Int aiTimerID)
		GoToState("waiting")
	EndEvent
EndState
