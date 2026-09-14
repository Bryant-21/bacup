Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)
    If akActor == Game.GetPlayer() && W05_MQS_202P_Acrobat != None && !W05_MQS_202P_Acrobat.IsStageDone(150)
        W05_MQS_202P_Acrobat.SetStage(150)
    EndIf
EndFunction
