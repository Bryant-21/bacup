; TODO

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue1_ActorValue != None && playerRef.GetValue(W05_Clue1_ActorValue) < 1.0
        If pW05_Clue1_CoverLetter != None && playerRef.GetItemCount(pW05_Clue1_CoverLetter) == 0
            playerRef.AddItem(pW05_Clue1_CoverLetter, 1, False)
        EndIf
        If pW05_Clue1_Invoice01 != None && playerRef.GetItemCount(pW05_Clue1_Invoice01) == 0
            playerRef.AddItem(pW05_Clue1_Invoice01, 1, False)
        EndIf
        If pW05_Clue1_Invoice02 != None && playerRef.GetItemCount(pW05_Clue1_Invoice02) == 0
            playerRef.AddItem(pW05_Clue1_Invoice02, 1, False)
        EndIf
        playerRef.SetValue(W05_Clue1_ActorValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue1_ActorValue != None && playerRef.GetValue(W05_Clue1_ActorValue) < 2.0
        If pW05_Package01 != None && playerRef.GetItemCount(pW05_Package01) > 0
            playerRef.RemoveItem(pW05_Package01, 1, True)
        EndIf
        If pW05_Clue1_CoverLetter != None && playerRef.GetItemCount(pW05_Clue1_CoverLetter) > 0
            playerRef.RemoveItem(pW05_Clue1_CoverLetter, 1, True)
        EndIf
        If pW05_Clue1_Invoice01 != None && playerRef.GetItemCount(pW05_Clue1_Invoice01) > 0
            playerRef.RemoveItem(pW05_Clue1_Invoice01, 1, True)
        EndIf
        If pW05_Clue1_Invoice02 != None && playerRef.GetItemCount(pW05_Clue1_Invoice02) > 0
            playerRef.RemoveItem(pW05_Clue1_Invoice02, 1, True)
        EndIf
        playerRef.SetValue(W05_Clue1_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue2_ActorValue != None && playerRef.GetValue(W05_Clue2_ActorValue) < 1.0
        If pW05_Clue2_CoverLetter != None && playerRef.GetItemCount(pW05_Clue2_CoverLetter) == 0
            playerRef.AddItem(pW05_Clue2_CoverLetter, 1, False)
        EndIf
        If pW05_Clue2_WeighStation != None && playerRef.GetItemCount(pW05_Clue2_WeighStation) == 0
            playerRef.AddItem(pW05_Clue2_WeighStation, 1, False)
        EndIf
        playerRef.SetValue(W05_Clue2_ActorValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue2_ActorValue != None && playerRef.GetValue(W05_Clue2_ActorValue) < 2.0
        If pW05_Package02 != None && playerRef.GetItemCount(pW05_Package02) > 0
            playerRef.RemoveItem(pW05_Package02, 1, True)
        EndIf
        If pW05_Clue2_CoverLetter != None && playerRef.GetItemCount(pW05_Clue2_CoverLetter) > 0
            playerRef.RemoveItem(pW05_Clue2_CoverLetter, 1, True)
        EndIf
        If pW05_Clue2_WeighStation != None && playerRef.GetItemCount(pW05_Clue2_WeighStation) > 0
            playerRef.RemoveItem(pW05_Clue2_WeighStation, 1, True)
        EndIf
        playerRef.SetValue(W05_Clue2_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue3_ActorValue != None && playerRef.GetValue(W05_Clue3_ActorValue) < 1.0
        If pW05_Clue3_CoverLetter != None && playerRef.GetItemCount(pW05_Clue3_CoverLetter) == 0
            playerRef.AddItem(pW05_Clue3_CoverLetter, 1, False)
        EndIf
        If pW05_Clue3_Death1 != None && playerRef.GetItemCount(pW05_Clue3_Death1) == 0
            playerRef.AddItem(pW05_Clue3_Death1, 1, False)
        EndIf
        If pW05_Clue3_Death2 != None && playerRef.GetItemCount(pW05_Clue3_Death2) == 0
            playerRef.AddItem(pW05_Clue3_Death2, 1, False)
        EndIf
        If pW05_Clue3_Death3 != None && playerRef.GetItemCount(pW05_Clue3_Death3) == 0
            playerRef.AddItem(pW05_Clue3_Death3, 1, False)
        EndIf
        If pW05_Clue3_DeathOverview != None && playerRef.GetItemCount(pW05_Clue3_DeathOverview) == 0
            playerRef.AddItem(pW05_Clue3_DeathOverview, 1, False)
        EndIf
        playerRef.SetValue(W05_Clue3_ActorValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue3_ActorValue != None && playerRef.GetValue(W05_Clue3_ActorValue) < 2.0
        If pW05_Package03 != None && playerRef.GetItemCount(pW05_Package03) > 0
            playerRef.RemoveItem(pW05_Package03, 1, True)
        EndIf
        If pW05_Clue3_CoverLetter != None && playerRef.GetItemCount(pW05_Clue3_CoverLetter) > 0
            playerRef.RemoveItem(pW05_Clue3_CoverLetter, 1, True)
        EndIf
        If pW05_Clue3_Death1 != None && playerRef.GetItemCount(pW05_Clue3_Death1) > 0
            playerRef.RemoveItem(pW05_Clue3_Death1, 1, True)
        EndIf
        If pW05_Clue3_Death2 != None && playerRef.GetItemCount(pW05_Clue3_Death2) > 0
            playerRef.RemoveItem(pW05_Clue3_Death2, 1, True)
        EndIf
        If pW05_Clue3_Death3 != None && playerRef.GetItemCount(pW05_Clue3_Death3) > 0
            playerRef.RemoveItem(pW05_Clue3_Death3, 1, True)
        EndIf
        If pW05_Clue3_DeathOverview != None && playerRef.GetItemCount(pW05_Clue3_DeathOverview) > 0
            playerRef.RemoveItem(pW05_Clue3_DeathOverview, 1, True)
        EndIf
        playerRef.SetValue(W05_Clue3_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue4_ActorValue != None && playerRef.GetValue(W05_Clue4_ActorValue) < 1.0
        If pW05_Clue4_CoverLetter != None && playerRef.GetItemCount(pW05_Clue4_CoverLetter) == 0
            playerRef.AddItem(pW05_Clue4_CoverLetter, 1, False)
        EndIf
        If pW05_Clue4_FenceReport != None && playerRef.GetItemCount(pW05_Clue4_FenceReport) == 0
            playerRef.AddItem(pW05_Clue4_FenceReport, 1, False)
        EndIf
        If pW05_Clue4_Holotape != None && Alias_04_Proof_Holotape.GetReference() == None && playerRef.GetItemCount(pW05_Clue4_Holotape) == 0
            ObjectReference proofTape = playerRef.PlaceAtMe(pW05_Clue4_Holotape, 1, True)
            If proofTape
                Alias_04_Proof_Holotape.ForceRefTo(proofTape)
                playerRef.AddItem(proofTape, 1, False)
            EndIf
        EndIf
        playerRef.SetValue(W05_Clue4_ActorValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue4_ActorValue != None && playerRef.GetValue(W05_Clue4_ActorValue) < 2.0
        If pW05_Package04 != None && playerRef.GetItemCount(pW05_Package04) > 0
            playerRef.RemoveItem(pW05_Package04, 1, True)
        EndIf
        If pW05_Clue4_CoverLetter != None && playerRef.GetItemCount(pW05_Clue4_CoverLetter) > 0
            playerRef.RemoveItem(pW05_Clue4_CoverLetter, 1, True)
        EndIf
        If pW05_Clue4_FenceReport != None && playerRef.GetItemCount(pW05_Clue4_FenceReport) > 0
            playerRef.RemoveItem(pW05_Clue4_FenceReport, 1, True)
        EndIf
        playerRef.SetValue(W05_Clue4_ActorValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue5_ActorValue != None && playerRef.GetValue(W05_Clue5_ActorValue) < 1.0
        If pW05_Clue5_CoverLetter != None && playerRef.GetItemCount(pW05_Clue5_CoverLetter) == 0
            playerRef.AddItem(pW05_Clue5_CoverLetter, 1, False)
        EndIf
        If pW05_Clue5_Newspaper01 != None && playerRef.GetItemCount(pW05_Clue5_Newspaper01) == 0
            playerRef.AddItem(pW05_Clue5_Newspaper01, 1, False)
        EndIf
        If pW05_Clue5_Newspaper02 != None && playerRef.GetItemCount(pW05_Clue5_Newspaper02) == 0
            playerRef.AddItem(pW05_Clue5_Newspaper02, 1, False)
        EndIf
        If pW05_Clue5_PoliceReport != None && playerRef.GetItemCount(pW05_Clue5_PoliceReport) == 0
            playerRef.AddItem(pW05_Clue5_PoliceReport, 1, False)
        EndIf
        If pW05_Clue5_Overview != None && playerRef.GetItemCount(pW05_Clue5_Overview) == 0
            playerRef.AddItem(pW05_Clue5_Overview, 1, False)
        EndIf
        playerRef.SetValue(W05_Clue5_ActorValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    If W05_Clue5_ActorValue != None && playerRef.GetValue(W05_Clue5_ActorValue) < 2.0
        If pW05_Package05 != None && playerRef.GetItemCount(pW05_Package05) > 0
            playerRef.RemoveItem(pW05_Package05, 1, True)
        EndIf
        If pW05_Clue5_CoverLetter != None && playerRef.GetItemCount(pW05_Clue5_CoverLetter) > 0
            playerRef.RemoveItem(pW05_Clue5_CoverLetter, 1, True)
        EndIf
        If pW05_Clue5_Newspaper01 != None && playerRef.GetItemCount(pW05_Clue5_Newspaper01) > 0
            playerRef.RemoveItem(pW05_Clue5_Newspaper01, 1, True)
        EndIf
        If pW05_Clue5_Newspaper02 != None && playerRef.GetItemCount(pW05_Clue5_Newspaper02) > 0
            playerRef.RemoveItem(pW05_Clue5_Newspaper02, 1, True)
        EndIf
        If pW05_Clue5_PoliceReport != None && playerRef.GetItemCount(pW05_Clue5_PoliceReport) > 0
            playerRef.RemoveItem(pW05_Clue5_PoliceReport, 1, True)
        EndIf
        If pW05_Clue5_Overview != None && playerRef.GetItemCount(pW05_Clue5_Overview) > 0
            playerRef.RemoveItem(pW05_Clue5_Overview, 1, True)
        EndIf
        playerRef.SetValue(W05_Clue5_ActorValue, 2.0)
    EndIf
EndFunction
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
