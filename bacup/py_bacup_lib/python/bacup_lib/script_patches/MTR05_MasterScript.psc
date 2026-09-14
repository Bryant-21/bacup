Function TriggerMotherlodeBreach()
    If MTR05_Mother.IsStageDone(iMotherlodeStandardSceneStage)
        MTR05_Mother.SetStage(iMotherlodeAltSceneStage)
    Else
        MTR05_Mother.SetStage(iMotherlodeStandardSceneStage)
    EndIf
EndFunction
