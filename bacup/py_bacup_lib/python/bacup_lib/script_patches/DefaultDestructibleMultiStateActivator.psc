Event OnDestructionStageChanged(int aiOldStage, int aiCurrentStage)
	If aiCurrentStage > 0
		SetLocalStateByName(DestroyedStateName)
		GoToState("Destroyed")
	EndIf
EndEvent

Function SetLocalStateByName(String stateName)
	If stateName == "" || AnimationStates == None
		Return
	EndIf
	AnimationState[] states = AnimationStates
	Int i = 0
	While i < states.Length
		If states[i].StateName == stateName
			SetLocalState(i)
			i = states.Length
		Else
			i += 1
		EndIf
	EndWhile
EndFunction

State Destroyed
	Event OnDestructionStageChanged(int aiOldStage, int aiCurrentStage)
		If aiCurrentStage == 0 && aiOldStage > 0
			If ShouldRepairToFixedStateName != ""
				SetLocalStateByName(ShouldRepairToFixedStateName)
			Else
				SetLocalStateByName(StartStateName)
			EndIf
			GoToState("")
		EndIf
	EndEvent
EndState
