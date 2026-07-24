Function Fragment_Stage_0020_Item_00()
    If W05_MQ_101P && !W05_MQ_101P.IsRunning()
        W05_MQ_101P.Start()
    EndIf
    If W05_MQ_101P && !W05_MQ_101P.IsStageDone(20)
        W05_MQ_101P.SetStage(20)
    EndIf
EndFunction
