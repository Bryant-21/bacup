Function ApplySiloState()
    DefaultMultiStateActivator stateController = Self as DefaultMultiStateActivator
    If stateController == None || stateController.AnimationStates == None || SiloStateGlobal == None
        Return
    EndIf

    String stateName = CONST_AnimationState_Off
    Int siloState = SiloStateGlobal.GetValueInt()
    If siloState == CONST_SiloState_Available
        stateName = CONST_AnimationState_On
    ElseIf siloState == CONST_SiloState_Launching
        stateName = CONST_AnimationState_Blink
    EndIf

    Int i = 0
    While i < stateController.AnimationStates.Length
        If stateController.AnimationStates[i].StateName == stateName
            stateController.SetLocalState(i, False)
            Return
        EndIf
        i += 1
    EndWhile
EndFunction

Event OnLoad()
    Utility.Wait(0.1)
    ApplySiloState()
EndEvent
