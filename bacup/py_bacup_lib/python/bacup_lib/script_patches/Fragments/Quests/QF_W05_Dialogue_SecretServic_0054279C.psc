Function Fragment_Stage_0000_Item_00()
    Actor playerActor = Alias_Player.GetActorReference()
    Actor reginaldActor = Alias_ReginaldStone.GetActorReference()
    If playerActor == None || reginaldActor == None
        Return
    EndIf
    If W05_MQA_206P != None && !W05_MQA_206P.IsRunning()
        Return
    EndIf
    playerActor.SetValue(W05_MQS_206P_Checkpoint, 1.0)
EndFunction
