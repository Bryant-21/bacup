Event OnQuestInit()
    InitializeLocalOfficers()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        B21RestoringOfficers = False
        InitializeLocalOfficers()
    EndIf
EndEvent

Function InitializeLocalOfficers()
    If B21RestoringOfficers || Scorched == None
        Return
    EndIf
    B21RestoringOfficers = True
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    CodesInitialized = True
    Int i = 0
    While i < Scorched.Length && i < 8
        Nuke_CodesOfficerScript officerAlias = Scorched[i].scorchedAlias
        If officerAlias != None
            Actor officer = officerAlias.GetReference() as Actor
            If officer == None
                ; FO76's alternative creation aliases may already own the actor.
                Int alternative = 126 + i * 2
                Int endAlternative = alternative + 2
                While officer == None && alternative < endAlternative
                    ReferenceAlias created = GetAlias(alternative) as ReferenceAlias
                    If created != None
                        officer = created.GetReference() as Actor
                    EndIf
                    alternative += 1
                EndWhile
                If officer == None && Scorched[i].spawnPointAlias != None
                    ObjectReference marker = Scorched[i].spawnPointAlias.GetReference()
                    ActorBase officerBase = Game.GetFormFromFile(0x003DA73A, "SeventySix.esm") as ActorBase
                    If marker != None && officerBase != None
                        officer = marker.PlaceActorAtMe(officerBase, ScorchedLevelModifier)
                    EndIf
                EndIf
                If officer != None
                    officerAlias.ForceRefTo(officer)
                EndIf
            EndIf
            If officer != None && !officer.IsDead()
                officerAlias.RestoreOfficer()
            EndIf
        EndIf
        i += 1
    EndWhile
    B21RestoringOfficers = False
EndFunction

Book Function ChooseLocalOfficerPage(Nuke_CodesOfficerScript officerAlias)
    Nuke_MasterScript master = Nuke_Master as Nuke_MasterScript
    If master == None || Scorched == None
        Return None
    EndIf
    Int i = 0
    While i < Scorched.Length && i < 8
        If Scorched[i].scorchedAlias == officerAlias
            Return master.LocalCodePage(Utility.RandomInt(0, 2), i)
        EndIf
        i += 1
    EndWhile
    Return None
EndFunction

ObjectReference Function PrepareLocalCodeTarget(Int aiSiloGroupID, Form akCodePage)
    Nuke_MasterScript master = Nuke_Master as Nuke_MasterScript
    If master == None || akCodePage == None || aiSiloGroupID < 0 || aiSiloGroupID > 2 || Scorched == None || Scorched.Length == 0
        Return None
    EndIf
    Int piece = 0
    While piece < 8 && master.LocalCodePage(aiSiloGroupID, piece) != akCodePage
        piece += 1
    EndWhile
    If piece == 8
        Return None
    EndIf
    InitializeLocalOfficers()
    Int startIndex = Utility.RandomInt(0, Scorched.Length - 1)
    Int i = 0
    While i < Scorched.Length && i < 8
        Int index = (startIndex + i) % Scorched.Length
        Nuke_CodesOfficerScript officerAlias = Scorched[index].scorchedAlias
        Actor officer
        If officerAlias != None
            officer = officerAlias.GetReference() as Actor
        EndIf
        If officer != None && !officer.IsDead()
            officerAlias.AssignLocalCodePage(akCodePage as Book)
            SiloGroupID = aiSiloGroupID
            Return officer
        EndIf
        i += 1
    EndWhile
    Return None
EndFunction
