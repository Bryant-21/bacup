Event OnQuestInit()
    ObjectReference holotapeContainer = Alias_HolotapeSpawnPoint.GetReference()
    ObjectReference holotapeRef = Alias_Holotape.GetReference()
    If holotapeRef == None && holotapeContainer != None && Form_Holotape != None
        holotapeRef = holotapeContainer.PlaceAtMe(Form_Holotape, 1, False, True, False)
        If holotapeRef != None
            Alias_Holotape.ForceRefTo(holotapeRef)
            holotapeContainer.AddItem(holotapeRef, 1, True)
            holotapeRef.Enable()
        EndIf
    EndIf

    ObjectReference supplySpawnPoint = Alias_SupplySpawnPoint.GetReference()
    ObjectReference suppliesRef = Alias_Supplies.GetReference()
    If suppliesRef == None && supplySpawnPoint != None && Form_FilterSupplies != None
        suppliesRef = supplySpawnPoint.PlaceAtMe(Form_FilterSupplies, 1, False, True, False)
        If suppliesRef != None
            Alias_Supplies.ForceRefTo(suppliesRef)
            suppliesRef.Enable()
        EndIf
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    ObjectReference player = Alias_Player.GetReference()

    If auiStageID == 200
        If player != None && CamShakeSpell != None
            CamShakeSpell.Cast(player, player)
        EndIf
    ElseIf auiStageID == 300
        holotapeFound = 1
    ElseIf auiStageID == 350
        suppliesFound = 1
        If player != None && NWOT_Beneath_HasSupplies != None
            player.SetValue(NWOT_Beneath_HasSupplies, 1.0)
        EndIf
    ElseIf auiStageID == 1000 || auiStageID == 1001 || auiStageID == 1002 || auiStageID == 1003 || auiStageID == 1100
        If player != None && NWOT_Pete_HasBeatenQuest != None
            player.SetValue(NWOT_Pete_HasBeatenQuest, 1.0)
        EndIf
    EndIf
EndEvent
