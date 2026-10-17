Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10, True)
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None && EN06_EnclavePresidentFaction != None
        playerRef.AddToFaction(EN06_EnclavePresidentFaction)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction
