Function Fragment_Stage_0500_Item_00()
    Actor player = Alias_owningPlayer.GetActorReference()
    If player != None
        player.SetValue(W05_MQ_101P_A_ShortVersionCompleted, 0.0)
    EndIf
EndFunction
