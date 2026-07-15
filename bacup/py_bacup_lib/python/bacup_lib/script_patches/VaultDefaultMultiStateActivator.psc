Function ApplyLocalState(Int stateIndex)
    If AnimationStates == None || stateIndex < 0 || stateIndex >= AnimationStates.Length
        Return
    EndIf

    If AnimationStates[stateIndex].EnableBlockActivation
        BlockActivation(AnimationStates[stateIndex].BlockActivation, AnimationStates[stateIndex].BlockActivationHideText)
        IsBlockingActivations = AnimationStates[stateIndex].BlockActivation
    ElseIf IsBlockingActivations
        BlockActivation(False, False)
        IsBlockingActivations = False
    EndIf
    If AnimationStates[stateIndex].EnableOverrideDisplayName
        SetOverrideName(AnimationStates[stateIndex].OverrideDisplayName)
    EndIf
    If AnimationStates[stateIndex].EnableOverrideActivateText
        SetActivateTextOverride(AnimationStates[stateIndex].OverrideActivateText)
    EndIf
EndFunction

Function SetLocalState(Int stateIndex, Bool playTransition = True)
    If lock_SetAnimationState || AnimationStates == None || stateIndex < 0 || stateIndex >= AnimationStates.Length
        Return
    EndIf

    lock_SetAnimationState = True
    Bool stateChanged = CurrentStateIndex != stateIndex
    CurrentStateIndex = stateIndex

    If Is3DLoaded()
        If stateChanged && playTransition
            If AnimationStates[stateIndex].StateAdditionalAnim != ""
                PlayAnimation(AnimationStates[stateIndex].StateAdditionalAnim)
            EndIf
            If AnimationStates[stateIndex].StateStartAnim != ""
                PlayAnimation(AnimationStates[stateIndex].StateStartAnim)
            ElseIf AnimationStates[stateIndex].StateJumpAnim != ""
                PlayAnimation(AnimationStates[stateIndex].StateJumpAnim)
            EndIf
        ElseIf AnimationStates[stateIndex].StateJumpAnim != ""
            PlayAnimation(AnimationStates[stateIndex].StateJumpAnim)
        EndIf
    EndIf

    ApplyLocalState(stateIndex)
    GoToState("ClientHasAnimated")
    lock_SetAnimationState = False
EndFunction

Function UpdateNetworkState()
    If AnimationStates == None || AnimationStates.Length == 0
        Return
    EndIf
    Int stateIndex = CurrentStateIndex
    If stateIndex < 0 || stateIndex >= AnimationStates.Length
        stateIndex = 0
    EndIf
    SetLocalState(stateIndex, False)
EndFunction

Event OnInit()
    UpdateNetworkState()
EndEvent

Event OnSimpleNetworkStateSet()
    SetLocalState(CurrentStateIndex, True)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If AnimationStates != None && AnimationStates.Length > 0
        Int stateIndex = aiCurrentStage
        If stateIndex >= AnimationStates.Length
            stateIndex = AnimationStates.Length - 1
        EndIf
        SetLocalState(stateIndex, True)
    EndIf
EndEvent
