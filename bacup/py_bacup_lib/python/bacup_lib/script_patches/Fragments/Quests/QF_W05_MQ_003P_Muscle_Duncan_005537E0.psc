; The quest's four stages are all driven by DefaultAliasInventoryManagement
; instances sitting on alias 9 (owningPlayer), each watching one robot-room key:
;   stage 100 <- ProtectronRoomKey 41A33B    stage 200 <- AssaultronRoomKey 41A33A
;   stage 300 <- HandyRoomKey      41A33C    stage  10 <- DefaultSetStageOnInstanceLoadQuest
; Each key stage releases the world reference the matching alias was holding, so
; DefaultQuestRemovePlayersScript can clean the placed key up once it is looted.
Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef == None
        Return
    EndIf
    ; D&D Robotics is an instanced interior; FO76 rebuilt it on every entry, which
    ; can leave a returning player without the Protectron key they already earned.
    ; Re-issue strictly what stage 100 recorded as earned, so this can never hand
    ; out access the player has not actually worked for, and never double-grants.
    If W05_MQ_003P_Muscle_PlayerAcquiredProtectronKey && W05_MQ_003P_Muscle_ProtectronRoomKey
        If playerRef.GetValue(W05_MQ_003P_Muscle_PlayerAcquiredProtectronKey) >= 1.0
            If playerRef.GetItemCount(W05_MQ_003P_Muscle_ProtectronRoomKey as Form) == 0
                playerRef.AddItem(W05_MQ_003P_Muscle_ProtectronRoomKey as Form, 1, True)
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Alias_ProtectronKey.Clear()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_003P_Muscle_PlayerAcquiredProtectronKey
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerAcquiredProtectronKey, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Alias_AssaultronKey.Clear()
EndFunction

Function Fragment_Stage_0300_Item_00()
    Alias_HandyKey.Clear()
EndFunction
