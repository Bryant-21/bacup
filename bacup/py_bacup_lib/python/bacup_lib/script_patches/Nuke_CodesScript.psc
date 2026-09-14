Event OnQuestInit()
    CodesInitialized = True
    Int i = 0
    While i < Scorched.Length
        Nuke_CodesOfficerScript officerAlias = Scorched[i].scorchedAlias
        If officerAlias != None
            officerAlias.RestoreOfficer()
        EndIf
        i += 1
    EndWhile
EndEvent

ObjectReference Function PrepareLocalCodeTarget(Int aiSiloGroupID, Form akCodePage)
    If akCodePage == None || Scorched.Length == 0
        Return None
    EndIf
    Int startIndex = Utility.RandomInt(0, Scorched.Length - 1)
    Int i = 0
    While i < Scorched.Length
        Int index = (startIndex + i) % Scorched.Length
        Nuke_CodesOfficerScript officerAlias = Scorched[index].scorchedAlias
        ObjectReference officerRef
        If officerAlias != None
            officerRef = officerAlias.GetReference()
        EndIf
        If officerRef != None
            If officerRef.GetItemCount(akCodePage) < 1
                officerRef.AddItem(akCodePage, 1, True)
            EndIf
            SiloGroupID = aiSiloGroupID
            Return officerRef
        EndIf
        i += 1
    EndWhile
    Return None
EndFunction
