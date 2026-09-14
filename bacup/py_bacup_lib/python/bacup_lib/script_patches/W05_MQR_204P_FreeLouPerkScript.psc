Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)
    If akActor != Game.GetPlayer() || akTargetRef == None || W05_MQR_204P == None
        Return
    EndIf
    If W05_MQR_204P.GetStage() == 200 && !W05_MQR_204P.IsStageDone(300)
        W05_MQR_204P.SetStage(300)
    EndIf
EndFunction
