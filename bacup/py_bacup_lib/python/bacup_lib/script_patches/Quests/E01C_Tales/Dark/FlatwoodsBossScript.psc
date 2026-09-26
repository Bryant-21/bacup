Event OnAliasInit()
    B21Vulnerable = False
    B21LastInvulnerableMessageTime = 0.0
EndEvent

; Ghosting stands in for FO76's damage immunity; hits pass through while the clones protect the monster.
Function ApplyVulnerability(Bool abVulnerable)
    Actor firstBoss = None
    Int index = 0
    Int bossCount = GetCount()
    While index < bossCount
        Actor boss = GetAt(index) as Actor
        If boss != None && !boss.IsDead()
            If firstBoss == None
                firstBoss = boss
            EndIf
            If boss.IsGhost() == abVulnerable
                boss.SetGhost(!abVulnerable)
            EndIf
        EndIf
        index += 1
    EndWhile
    If firstBoss == None
        Return
    EndIf
    If abVulnerable && !B21Vulnerable
        If NPCFlatwoodsPain != None
            NPCFlatwoodsPain.Play(firstBoss)
        EndIf
    ElseIf !abVulnerable && B21LastInvulnerableMessageTime <= 0.0
        ShowInvulnerableMessage()
    EndIf
    B21Vulnerable = abVulnerable
EndFunction

Function ShowInvulnerableMessage()
    If E01C_Tales_Dark_InvulnerableMessage == None
        Return
    EndIf
    Float now = Utility.GetCurrentRealTime()
    If B21LastInvulnerableMessageTime > 0.0 && now >= B21LastInvulnerableMessageTime && now - B21LastInvulnerableMessageTime < TimeBetweenInvulnerableMessages as Float
        Return
    EndIf
    B21LastInvulnerableMessageTime = now
    E01C_Tales_Dark_InvulnerableMessage.Show()
EndFunction
