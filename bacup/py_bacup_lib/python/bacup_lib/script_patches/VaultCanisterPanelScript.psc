Function UpdateNetworkState(Bool isClientUpdateOnLoad)
	If lock_UpdateNetworkState
		Return
	EndIf

	lock_UpdateNetworkState = True
	If AnimationStates == None || AnimationStates.Length == 0
		lock_UpdateNetworkState = False
		Return
	EndIf

	Int stateIndex = CurrentStateIndex
	If stateIndex < 0 || stateIndex >= AnimationStates.Length
		stateIndex = 0
	EndIf
	CurrentStateIndex = stateIndex

	If Is3DLoaded()
		If AnimationStates[stateIndex].StateAdditionalAnim != ""
			PlayAnimation(AnimationStates[stateIndex].StateAdditionalAnim)
		EndIf
		If !isClientUpdateOnLoad && AnimationStates[stateIndex].StateStartAnim != ""
			PlayAnimation(AnimationStates[stateIndex].StateStartAnim)
		ElseIf AnimationStates[stateIndex].StateJumpAnim != ""
			PlayAnimation(AnimationStates[stateIndex].StateJumpAnim)
		EndIf
	EndIf

	If AnimationStates[stateIndex].EnableBlockActivation
		BlockActivation(AnimationStates[stateIndex].BlockActivation, AnimationStates[stateIndex].BlockActivationHideText)
		IsBlockingActivations = AnimationStates[stateIndex].BlockActivation
	ElseIf IsBlockingActivations
		BlockActivation(False, False)
		IsBlockingActivations = False
	EndIf
	If AnimationStates[stateIndex].EnableOverrideActivateText
		SetActivateTextOverride(AnimationStates[stateIndex].OverrideActivateText)
	EndIf

	GoToState("ClientHasAnimated")
	lock_UpdateNetworkState = False
EndFunction

Event OnInit()
	UpdateNetworkState(True)
EndEvent
