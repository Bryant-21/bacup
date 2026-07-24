Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == 9000 && W05_Community_RaiderFishCamp_Quest != None && Players != None
        SQ = W05_Community_RaiderFishCamp_Quest as w05_com_rfc_participants_qi
        If SQ != None
            int i = 0
            While i < Players.GetCount()
                ObjectReference participant = Players.GetAt(i)
                If participant != None && SQ.CurrentPlayerParticipants != None && SQ.CompletedPlayerParticipants != None
                    SQ.CurrentPlayerParticipants.RemoveRef(participant)
                    SQ.CompletedPlayerParticipants.AddRef(participant)
                EndIf
                i += 1
            EndWhile
        EndIf
    EndIf
EndEvent
