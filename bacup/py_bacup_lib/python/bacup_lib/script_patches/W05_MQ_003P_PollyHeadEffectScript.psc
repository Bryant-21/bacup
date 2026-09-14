Event OnEffectStart(Actor akTarget, Actor akCaster)
    OwningPlayer = akTarget
    OwningInstance = W05_MQ_003P_Muscle
    If OwningPlayer && OwningInstance && OwningInstance.IsRunning() && !OwningInstance.IsStageDone(ShutdownStage)
        If OwningInstance.IsStageDone(1020) && !OwningInstance.IsStageDone(1050)
            OwningInstance.SetStage(1050)
        EndIf
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    OwningPlayer = None
    OwningInstance = None
EndEvent
