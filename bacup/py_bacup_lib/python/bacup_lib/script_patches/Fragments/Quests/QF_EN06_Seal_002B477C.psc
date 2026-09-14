Function Fragment_Stage_0110_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None && EN06_EnclavePresidentFaction != None
        playerRef.AddToFaction(EN06_EnclavePresidentFaction)
    EndIf
EndFunction
