; TODO

Function Fragment_Stage_1100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map1_ActorValue != None && playerRef.GetValue(W05_Map1_ActorValue) < 1.0
        playerRef.SetValue(W05_Map1_ActorValue, 1.0)
        If pW05_Message_Topo01 != None
            pW05_Message_Topo01.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map1_ActorValue != None && playerRef.GetValue(W05_Map1_ActorValue) < 2.0
        If pW05_Topo01 != None && playerRef.GetItemCount(pW05_Topo01) > 0
            playerRef.RemoveItem(pW05_Topo01, 1, True)
        EndIf
        playerRef.SetValue(W05_Map1_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map2_ActorValue != None && playerRef.GetValue(W05_Map2_ActorValue) < 1.0
        playerRef.SetValue(W05_Map2_ActorValue, 1.0)
        If pW05_Message_Topo02 != None
            pW05_Message_Topo02.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map2_ActorValue != None && playerRef.GetValue(W05_Map2_ActorValue) < 2.0
        If pW05_Topo02 != None && playerRef.GetItemCount(pW05_Topo02) > 0
            playerRef.RemoveItem(pW05_Topo02, 1, True)
        EndIf
        playerRef.SetValue(W05_Map2_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map3_ActorValue != None && playerRef.GetValue(W05_Map3_ActorValue) < 1.0
        playerRef.SetValue(W05_Map3_ActorValue, 1.0)
        If pW05_Message_Topo03 != None
            pW05_Message_Topo03.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1350_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map3_ActorValue != None && playerRef.GetValue(W05_Map3_ActorValue) < 2.0
        If pW05_Topo03 != None && playerRef.GetItemCount(pW05_Topo03) > 0
            playerRef.RemoveItem(pW05_Topo03, 1, True)
        EndIf
        playerRef.SetValue(W05_Map3_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map4_ActorValue != None && playerRef.GetValue(W05_Map4_ActorValue) < 1.0
        playerRef.SetValue(W05_Map4_ActorValue, 1.0)
        If pW05_Message_Topo04 != None
            pW05_Message_Topo04.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map4_ActorValue != None && playerRef.GetValue(W05_Map4_ActorValue) < 2.0
        If pW05_Topo04 != None && playerRef.GetItemCount(pW05_Topo04) > 0
            playerRef.RemoveItem(pW05_Topo04, 1, True)
        EndIf
        playerRef.SetValue(W05_Map4_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map5_ActorValue != None && playerRef.GetValue(W05_Map5_ActorValue) < 1.0
        playerRef.SetValue(W05_Map5_ActorValue, 1.0)
        If pW05_Message_Topo05 != None
            pW05_Message_Topo05.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1550_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map5_ActorValue != None && playerRef.GetValue(W05_Map5_ActorValue) < 2.0
        If pW05_Topo05 != None && playerRef.GetItemCount(pW05_Topo05) > 0
            playerRef.RemoveItem(pW05_Topo05, 1, True)
        EndIf
        playerRef.SetValue(W05_Map5_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map6_ActorValue != None && playerRef.GetValue(W05_Map6_ActorValue) < 1.0
        playerRef.SetValue(W05_Map6_ActorValue, 1.0)
        If pW05_Message_Topo06 != None
            pW05_Message_Topo06.Show()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1650_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Map6_ActorValue != None && playerRef.GetValue(W05_Map6_ActorValue) < 2.0
        If pW05_Topo06 != None && playerRef.GetItemCount(pW05_Topo06) > 0
            playerRef.RemoveItem(pW05_Topo06, 1, True)
        EndIf
        playerRef.SetValue(W05_Map6_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1999_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None || pW05_MQ00_CodeAV == None
        Return
    EndIf

    Int mapCode = playerRef.GetValue(pW05_MQ00_CodeAV) as Int
    If mapCode < 100000 || mapCode > 999999
        playerRef.SetValue(pW05_MQ00_CodeAV, Utility.RandomInt(100000, 999999))
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_PlayerKnows_BeenToVault79, 1.0)
    EndIf
    If IsStageDone(2200) && !IsStageDone(2300)
        SetStage(2300)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pW05_MQ00_CodeAV, 1.0)
    EndIf
    If IsStageDone(2100) && !IsStageDone(2300)
        SetStage(2300)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pW05_MQ00_Completed, 1.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction
