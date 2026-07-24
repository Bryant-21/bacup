Function Fragment_Stage_0000_Item_00()
    If W05_MQA_206P != None && W05_MQA_206P.IsCompleted()
        Actor player = Alias_OwningPlayer.GetActorReference()
        If player != None
            If W05_MQR_RaRaAwayValue != None
                player.SetValue(W05_MQR_RaRaAwayValue, 0.0)
            EndIf
            If W05_MQR_MegAwayValue != None
                player.SetValue(W05_MQR_MegAwayValue, 0.0)
            EndIf
            If W05_MQR_LouAwayValue != None
                player.SetValue(W05_MQR_LouAwayValue, 0.0)
            EndIf
            If W05_MQR_JohnnyAwayValue != None
                player.SetValue(W05_MQR_JohnnyAwayValue, 0.0)
            EndIf
            If W05_MQR_GailAwayValue != None
                player.SetValue(W05_MQR_GailAwayValue, 0.0)
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor player = Alias_OwningPlayer.GetActorReference()
    If player != None && player.GetValue(W05_PostMQ_ModRepTrackingAV_Meg) == 0.0
        player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Large.GetValue())
        player.SetValue(W05_PostMQ_ModRepTrackingAV_Meg, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
    Actor player = Alias_OwningPlayer.GetActorReference()
    If player != None && player.GetValue(W05_PostMQ_ModRepTrackingAV_Meg) == 0.0
        player.ModValue(Reputation_AV_Crater, Rep_Mod_Subtract_Small.GetValue())
        player.SetValue(W05_PostMQ_ModRepTrackingAV_Meg, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    Actor player = Alias_OwningPlayer.GetActorReference()
    If player != None && player.GetValue(W05_PostMQ_ModRepTrackingAV_Gail_Parent) == 0.0
        player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Medium.GetValue())
        player.SetValue(W05_PostMQ_ModRepTrackingAV_Gail_Parent, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0021_Item_00()
    Actor player = Alias_OwningPlayer.GetActorReference()
    If player != None && player.GetValue(W05_PostMQ_ModRepTrackingAV_Gail_Stupid) == 0.0
        player.ModValue(Reputation_AV_Crater, Rep_Mod_Subtract_Medium.GetValue())
        player.SetValue(W05_PostMQ_ModRepTrackingAV_Gail_Stupid, 1.0)
    EndIf
EndFunction
