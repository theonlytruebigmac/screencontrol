'use client';

/**
 * Chat panel component — session chat between operator and agent.
 *
 * Renders messages, input field, and sends ChatMessage payloads.
 */

import { useState, useRef, useEffect, useCallback, FormEvent } from 'react';
import { Send, MessageSquare, User, Bot } from 'lucide-react';

interface ChatMessageData {
    id: string;
    senderId: string;
    senderName: string;
    content: string;
    timestamp: Date;
    isLocal: boolean;
}

interface ChatPanelProps {
    sessionId: string;
    userName?: string;
    onSendMessage?: (content: string) => void;
    className?: string;
}

export default function ChatPanel({ sessionId, userName = 'Operator', onSendMessage, className }: ChatPanelProps) {
    const [messages, setMessages] = useState<ChatMessageData[]>([
        {
            id: 'system-1',
            senderId: 'system',
            senderName: 'System',
            content: `Chat session started. Connected to session ${sessionId.slice(0, 8)}…`,
            timestamp: new Date(),
            isLocal: false,
        },
    ]);
    const [input, setInput] = useState('');
    const scrollRef = useRef<HTMLDivElement>(null);

    // Auto-scroll to bottom on new messages
    useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
    }, [messages]);

    const handleSend = useCallback((e: FormEvent) => {
        e.preventDefault();
        if (!input.trim()) return;

        const msg: ChatMessageData = {
            id: `local-${Date.now()}`,
            senderId: 'local',
            senderName: userName,
            content: input.trim(),
            timestamp: new Date(),
            isLocal: true,
        };

        setMessages((prev) => [...prev, msg]);
        onSendMessage?.(input.trim());
        setInput('');
    }, [input, userName, onSendMessage]);

    /** Add an incoming message (called by parent via ref or callback). */
    const addMessage = useCallback((senderId: string, senderName: string, content: string) => {
        const msg: ChatMessageData = {
            id: `remote-${Date.now()}`,
            senderId,
            senderName,
            content,
            timestamp: new Date(),
            isLocal: false,
        };
        setMessages((prev) => [...prev, msg]);
    }, []);

    return (
        <div className={`flex flex-col glass rounded-xl border border-gray-800 overflow-hidden ${className || ''}`}>
            {/* Header */}
            <div className="flex items-center gap-2 px-4 py-3 border-b border-gray-800 bg-gray-900/50">
                <MessageSquare className="w-4 h-4 text-emerald-400" />
                <span className="text-sm font-medium text-white">Chat</span>
                <span className="text-xs text-gray-500 ml-auto font-mono">{sessionId.slice(0, 8)}</span>
            </div>

            {/* Messages */}
            <div ref={scrollRef} className="flex-1 overflow-y-auto p-4 space-y-3 min-h-[200px] max-h-[500px]">
                {messages.map((msg) => (
                    <div key={msg.id} className={`flex gap-2.5 ${msg.isLocal ? 'flex-row-reverse' : ''}`}>
                        {/* Avatar */}
                        <div className={`w-7 h-7 rounded-full flex items-center justify-center text-xs shrink-0 ${msg.senderId === 'system'
                            ? 'bg-gray-800 text-gray-500'
                            : msg.isLocal
                                ? 'bg-[#e05246]/20 text-[#f06b60]'
                                : 'bg-cyan-500/20 text-cyan-400'
                            }`}>
                            {msg.senderId === 'system' ? (
                                <Bot className="w-3.5 h-3.5" />
                            ) : (
                                <User className="w-3.5 h-3.5" />
                            )}
                        </div>

                        {/* Bubble */}
                        <div className={`max-w-[75%] ${msg.isLocal ? 'text-right' : ''}`}>
                            <div className="flex items-center gap-2 mb-0.5">
                                <span className="text-xs font-medium text-gray-400">{msg.senderName}</span>
                                <span className="text-xs text-gray-600">
                                    {msg.timestamp.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}
                                </span>
                            </div>
                            <div className={`inline-block px-3 py-2 rounded-xl text-sm ${msg.senderId === 'system'
                                ? 'bg-gray-800/50 text-gray-400 italic'
                                : msg.isLocal
                                    ? 'bg-[#e05246]/20 text-white rounded-tr-sm'
                                    : 'bg-gray-800 text-gray-200 rounded-tl-sm'
                                }`}>
                                {msg.content}
                            </div>
                        </div>
                    </div>
                ))}
            </div>

            {/* Input */}
            <form onSubmit={handleSend} className="flex items-center gap-2 px-4 py-3 border-t border-gray-800 bg-gray-900/30">
                <input
                    type="text"
                    placeholder="Type a message..."
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    className="flex-1 px-3 py-2 bg-gray-900 border border-gray-800 rounded-lg text-sm text-white placeholder-gray-600 focus:outline-none focus:border-[#e05246]"
                />
                <button
                    type="submit"
                    disabled={!input.trim()}
                    className="p-2 bg-[#e05246] hover:bg-[#c43d32] disabled:opacity-30 disabled:cursor-not-allowed text-white rounded-lg transition-colors"
                >
                    <Send className="w-4 h-4" />
                </button>
            </form>
        </div>
    );
}
