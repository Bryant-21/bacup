Int Function FindAnimationStateIndex(String stateName)
	If stateName == "" || AnimationStates == None
		Return -1
	EndIf
	Int index = 0
	While index < AnimationStates.Length
		If AnimationStates[index].StateName == stateName
			Return index
		EndIf
		index += 1
	EndWhile
	Return -1
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	If aiCurrentStage > aiOldStage
		Int destroyedIndex = FindAnimationStateIndex(DestroyedStateName)
		If destroyedIndex >= 0
			SetLocalState(destroyedIndex, True)
		EndIf
	ElseIf aiCurrentStage == 0 && aiOldStage > 0
		repairToStateName = ShouldRepairToFixedStateName
		If repairToStateName == ""
			repairToStateName = UnusedStateName
		EndIf
		Int repairedIndex = FindAnimationStateIndex(repairToStateName)
		If repairedIndex >= 0
			SetLocalState(repairedIndex, True)
		EndIf
	EndIf
EndEvent
