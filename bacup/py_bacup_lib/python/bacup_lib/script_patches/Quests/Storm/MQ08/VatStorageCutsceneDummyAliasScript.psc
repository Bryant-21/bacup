Function StartCutscene()
	Quest owningQuest = GetOwningQuest()
	If owningQuest == None || owningQuest.IsStageDone(iStageToSetOnCompletion) || iCurrentCutsceneStage > 0
		Return
	EndIf

	iCurrentCutsceneStage = 1
	ObjectReference cutsceneRef = GetReference()
	While iCurrentCutsceneStage <= CutsceneStages.Length
		CutsceneStage currentStage = CutsceneStages[iCurrentCutsceneStage - 1]
		ObjectReference lightRef = cutsceneRef.GetLinkedRef(currentStage.LightKeyword)
		If lightRef
			lightRef.Enable()
		EndIf
		If currentStage.SoundToPlay
			currentStage.SoundToPlay.Play(cutsceneRef)
		EndIf
		Utility.Wait(currentStage.TimeToWait)
		iCurrentCutsceneStage += 1
	EndWhile

	If !owningQuest.IsStageDone(iStageToSetOnCompletion)
		owningQuest.SetStage(iStageToSetOnCompletion)
	EndIf
EndFunction
