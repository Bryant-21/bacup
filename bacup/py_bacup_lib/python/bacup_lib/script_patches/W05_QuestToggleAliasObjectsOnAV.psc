Function EvaluateEnableStates()
    Actor player = None
    If OwningPlayer != None
        player = OwningPlayer.GetActorReference()
    EndIf
    If player == None
        Return
    EndIf

    Int i = 0
    Int count = EnableStates.Length
    While i < count
        If EnableStates[i].SwapAV != None && EnableStates[i].SwapTarget != None
            ObjectReference targetRef = EnableStates[i].SwapTarget.GetReference()
            If targetRef != None
                Float currentValue = player.GetValue(EnableStates[i].SwapAV)
                Bool meetsTarget = currentValue >= EnableStates[i].TargetValue

                If meetsTarget
                    If EnableStates[i].EnableObj
                        targetRef.Enable()
                    Else
                        targetRef.Disable()
                    EndIf
                ElseIf !EnableStates[i].MaintainStateOnGreaterThanTargetValue
                    If EnableStates[i].EnableObj
                        targetRef.Disable()
                    Else
                        targetRef.Enable()
                    EndIf
                EndIf
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction
