; TODO
; Deferred: no FO4 replacement for the per-state display-name override.

Function DoUpdateState(Int stateIndex)
    If AnimationStates == None || stateIndex < 0 || stateIndex >= AnimationStates.Length
        Return
    EndIf

    ; EnableOverrideDisplayName/OverrideDisplayName have no Fallout 4 equivalent:
    ; per-state display-name override was FO76 ObjectReference.SetOverrideName.
    If AnimationStates[stateIndex].EnableOverrideActivateText
        SetActivateTextOverride(AnimationStates[stateIndex].OverrideActivateText)
    EndIf
    If AnimationStates[stateIndex].EnableBlockActivation
        BlockActivation(AnimationStates[stateIndex].BlockActivation, AnimationStates[stateIndex].BlockActivationHideText)
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
        If stateChanged && playTransition && AnimationStates[stateIndex].StateStartAnim != ""
            PlayAnimation(AnimationStates[stateIndex].StateStartAnim)
        ElseIf AnimationStates[stateIndex].StateJumpAnim != ""
            PlayAnimation(AnimationStates[stateIndex].StateJumpAnim)
        EndIf
    EndIf

    DoUpdateState(stateIndex)
    GoToState("main")
    lock_SetAnimationState = False
EndFunction

Function InitializeLocalState()
    If AnimationStates == None || AnimationStates.Length == 0
        Return
    EndIf

    Int initialIndex = CurrentStateIndex
    If StartStateName != ""
        Int i = 0
        While i < AnimationStates.Length
            If AnimationStates[i].StateName == StartStateName
                initialIndex = i
                i = AnimationStates.Length
            Else
                i += 1
            EndIf
        EndWhile
    EndIf

    If initialIndex < 0 || initialIndex >= AnimationStates.Length
        initialIndex = 0
    EndIf
    SetLocalState(initialIndex, False)
EndFunction

Event OnInit()
    InitializeLocalState()
EndEvent

; OnSimpleNetworkStateSet was raised by the FO76 server when a replicated state
; variable changed. Single-player has no replication: SetLocalState is already
; the authoritative entry point, so the handler is dropped rather than re-homed.
; @drop-member OnSimpleNetworkStateSet

Event OnLoad()
    InitializeLocalState()
EndEvent
