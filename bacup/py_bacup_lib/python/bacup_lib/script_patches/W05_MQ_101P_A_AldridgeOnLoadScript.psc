Event OnLoad()
    If W05_MQ_101P_A && W05_MQ_101P_A.IsStageDone(1300) && !W05_MQ_101P_A.IsStageDone(1400)
        W05_MQ_101P_A.SetStage(1400)
    EndIf
EndEvent
