; MTRZ01 lost key: once the player has the key (stage 100) the matching locker is stocked with the bound loot
; (pMTRZ01_Lost_Loot plus the pre-war boss trunk list) for the player to open at stage 200.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != 100
        Return
    EndIf
    ObjectReference locker = (GetAlias(2) as ReferenceAlias).GetReference()
    If locker == None
        Return
    EndIf
    If pMTRZ01_Lost_Loot != None
        locker.AddItem(pMTRZ01_Lost_Loot, 1, True)
    EndIf
    If pContainer_Loot_Trunk_Prewar_Boss != None
        locker.AddItem(pContainer_Loot_Trunk_Prewar_Boss, 1, True)
    EndIf
EndEvent
