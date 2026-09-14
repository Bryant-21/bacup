; OnSyncVariableNetworkChanged("currentState") was raised on FO76 clients when the
; server replicated a new currentState. Nothing raises it in Fallout 4, so the
; handler is re-homed into a local SetCurrentState() entry point that any caller
; (fragment, sibling script, or this script's own OnLoad) can drive.
; @drop-member OnSyncVariableNetworkChanged

Int Function ResolveStateIndex(Int stateIndex)
    If stateIndex < 0 || stateIndex >= States.Length
        stateIndex = StartState
        If stateIndex < 0 || stateIndex >= States.Length
            stateIndex = 0
        EndIf
    EndIf
    Return stateIndex
EndFunction

Function SetCurrentState(Int newState)
    If States == None || States.Length == 0
        Return
    EndIf

    currentState = newState
    Int stateIndex = ResolveStateIndex(currentState)

    If clientState != stateIndex && Is3DLoaded()
        clientState = stateIndex
        PlayAnimation(States[stateIndex].TransitionAnim)
    EndIf
EndFunction

Function AdvanceState()
    If States == None || States.Length == 0
        Return
    EndIf
    SetCurrentState((ResolveStateIndex(currentState) + 1) % States.Length)
EndFunction

Event OnLoad()
    If States == None || States.Length == 0
        Return
    EndIf

    Int stateIndex = ResolveStateIndex(currentState)
    clientState = stateIndex
    PlayAnimation(States[stateIndex].IdleAnim)
EndEvent
