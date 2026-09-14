Function Fragment_Entry_01(ObjectReference akTargetRef, Actor akActor)
    If akActor == Game.GetPlayer()
        Self.SetW05MQS201Stage(akActor)
    EndIf
EndFunction

Function SetW05MQS201Stage(Actor akPlayer)
    If akPlayer == Game.GetPlayer() && W05_MQS_201P_Industrialist != None && !W05_MQS_201P_Industrialist.IsStageDone(952)
        W05_MQS_201P_Industrialist.SetStage(952)
    EndIf
EndFunction
