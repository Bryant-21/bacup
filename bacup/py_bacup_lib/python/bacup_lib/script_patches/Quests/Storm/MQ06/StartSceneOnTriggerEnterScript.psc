Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef != PlayerAlias.GetReference()
		Return
	EndIf

	Quest owner = GetOwningQuest()
	If owner == None || !owner.IsRunning()
		Return
	EndIf
	If iPreReqStage != -1 && !owner.IsStageDone(iPreReqStage)
		Return
	EndIf
	If iTurnOffStage != -1 && owner.IsStageDone(iTurnOffStage)
		Return
	EndIf
	If !SceneToStart.IsPlaying()
		SceneToStart.Start()
	EndIf
EndEvent
