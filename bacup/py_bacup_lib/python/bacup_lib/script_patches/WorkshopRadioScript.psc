Event OnDestructionStageChanged(int aiOldStage, int aiCurrentStage)
    If DestroyedStage <= 0
        Return
    EndIf
    If aiCurrentStage >= DestroyedStage
        SetRadioOn(false)
    Else
        SetRadioOn(true)
    EndIf
EndEvent
