Event OnSyncVariableNetworkChanged(String varName)
    If varName != "currentState" || States == None || States.Length == 0
        Return
    EndIf

    Int stateIndex = currentState
    If stateIndex < 0 || stateIndex >= States.Length
        stateIndex = StartState
        If stateIndex < 0 || stateIndex >= States.Length
            stateIndex = 0
        EndIf
    EndIf

    If clientState != stateIndex && Is3DLoaded()
        clientState = stateIndex
        PlayAnimation(States[stateIndex].TransitionAnim)
    EndIf
EndEvent

Event OnLoad()
    If States == None || States.Length == 0
        Return
    EndIf

    Int stateIndex = currentState
    If stateIndex < 0 || stateIndex >= States.Length
        stateIndex = StartState
        If stateIndex < 0 || stateIndex >= States.Length
            stateIndex = 0
        EndIf
    EndIf

    clientState = stateIndex
    PlayAnimation(States[stateIndex].IdleAnim)
EndEvent
